#!/bin/bash
##
##  Script for generating bootstrap archives.
##

set -e

# Can be changed by using '--repository' option.
REPO_BASE_URL="https://dl.bintray.com/termux/termux-packages-24"

# Can be changed by using '--prefix' option.
TERMUX_PREFIX="/data/data/com.termux/files/usr"

read_package_list() {
	local architecture=$1
	local package_name

	echo "[*] Reading package list for '$architecture'..."

	while read -r -d $'\xFF' package; do
		package_name=$(echo "$package" | grep Package: | awk '{ print $2 }')
		PACKAGE_METADATA["$package_name"]=$package
	done < <(
				curl -Ls "${REPO_BASE_URL}/dists/stable/main/binary-${architecture}/Packages" | \
					sed -e "s/^$/\xFF/g"
				echo
				curl -Ls "${REPO_BASE_URL}/dists/stable/main/binary-all/Packages" | \
					sed -e "s/^$/\xFF/g"
			)
}

pull_package() {
	local package_name=$1
	local package_tmpdir="${BOOTSTRAP_TMPDIR}/${package_name}"
	mkdir -p "$package_tmpdir"

	local package_url
	package_url="$REPO_BASE_URL/$(echo "${PACKAGE_METADATA[${package_name}]}" | grep Filename: | awk '{ print $2 }')"

	local package_dependencies
	package_dependencies=$(
		while read -r -d ',' token; do
			echo "$token" | cut -d'|' -f1 | sed -E 's@\(.*\)@@'
		done <<< "$(echo "${PACKAGE_METADATA[${package_name}]}" | grep Depends: | sed -E 's@^Depends:@@')"
	)

	# Recursively handle dependencies.
	if [ -n "$package_dependencies" ]; then
		local dep
		for dep in $package_dependencies; do
			if [ ! -e "${BOOTSTRAP_TMPDIR}/${dep}" ]; then
				pull_package $dep
			fi
		done
		unset dep
	fi

	echo "[*] Downloading '$package_name'..."
	curl --fail --location --output "$package_tmpdir/package.deb" "$package_url"

	echo "[*] Extracting '$package_name'..."
	(cd "$package_tmpdir"
		ar x package.deb

		# data.tar may have extension different from .xz
		if [ -f "./data.tar.xz" ]; then
			data_archive="data.tar.xz"
		elif [ -f "./data.tar.gz" ]; then
			data_archive="data.tar.gz"
		else
			echo "No data.tar.* found in '$package_name'."
			exit 1
		fi

		# Do same for control.tar.
		if [ -f "./control.tar.xz" ]; then
			control_archive="control.tar.xz"
		elif [ -f "./control.tar.gz" ]; then
			control_archive="control.tar.gz"
		else
			echo "No control.tar.* found in '$package_name'."
			exit 1
		fi

		# Extract files.
		tar xf "$data_archive" -C "$BOOTSTRAP_ROOTFS"
		tar tf "$data_archive" > "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/info/${package_name}.list"

		# Generate checksums (md5).
		tar xf "$data_archive"
		find data -type f -print0 | xargs -0 -r md5sum | sed 's@^\.$@@g' > "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/info/${package_name}.md5sums"

		# Extract metadata.
		tar xf "$control_archive"
		{
			cat control
			echo "Status: install ok installed"
			echo
		} >> "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/status"

		# Additional data: conffiles & scripts
		local file
		for file in conffiles postinst postrm preinst prerm; do
			if [ -f "${PWD}/${file}" ]; then
				cp "$file" "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/info/${package_name}.${file}"
			fi
		done
	)
}

create_bootstrap_archive() {
	local architecture=$1

	echo "[*] Creating 'bootstrap-${architecture}.zip'..."

	# Do not store symlinks in bootstrap archive.
	# Instead, put all information to SYMLINKS.txt
	(cd "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}"
		while read -r -d '' link; do
			echo "$(readlink "$link")←${link}" >> SYMLINKS.txt
			rm -f "$link"
		done < <(find . -type l -print0)

		zip -r9 "${BOOTSTRAP_TMPDIR}/bootstrap-${architecture}.zip" ./*
	)

	mv -f "${BOOTSTRAP_TMPDIR}/bootstrap-${architecture}.zip" ./
}

show_usage() {
	echo
	echo "Usage: generate-bootstraps.sh [options]"
	echo
	echo "Generate bootstrap archives for Termux application."
	echo
	echo "Options:"
	echo
	echo " -h, --help              Show this help."
	echo
	echo " -p, --prefix PATH       Specify rootfs prefix absolute path."
	echo "                         Should be exactly same as in packages"
	echo "                         in the remote repository."
	echo
	echo " -r, --repository URL    Specify URL for APT repository from"
	echo "                         which packages will be downloaded."
	echo
	echo "Repository URL: ${REPO_BASE_URL}"
	echo "Prefix: ${TERMUX_PREFIX}"
	echo
}

while (($# > 0)); do
	case "$1" in
		-h|--help)
			show_usage
			exit 0
			;;
		-p|--prefix)
			if [ $# -gt 1 ] && [[ $2 != -* ]]; then
				TERMUX_PREFIX="$2"
				shift 1
			else
				echo "[!] Option '--repository' requires an argument."
				show_usage
				exit 1
			fi
			;;
		-r|--repository)
			if [ $# -gt 1 ] && [[ $2 != -* ]]; then
				REPO_BASE_URL="$2"
				shift 1
			else
				echo "[!] Option '--repository' requires an argument."
				show_usage
				exit 1
			fi
			;;
		*)
			echo "[!] Got unknown option '$1'"
			show_usage
			exit 1
			;;
	esac
	shift 1
done

declare -A PACKAGE_METADATA

for package_arch in aarch64 arm i686 x86_64; do
	BOOTSTRAP_TMPDIR=$(mktemp -d /tmp/bootstrap-tmp.XXXXXXXX)
	BOOTSTRAP_ROOTFS="$BOOTSTRAP_TMPDIR/rootfs-$package_arch"

	# Create initial directories for $TERMUX_PREFIX
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/etc/apt/apt.conf.d"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/etc/apt/preferences.d"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/tmp"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/cache/apt/archives/partial"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/info"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/triggers"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/updates"
	mkdir -p "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/log/apt"
	touch "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/available"
	touch "${BOOTSTRAP_ROOTFS}/${TERMUX_PREFIX}/var/lib/dpkg/status"

	# Read metadata for all available packages.
	read_package_list "$package_arch"

	# Download and extract specified packages and their dependencies.
	pull_package apt
	pull_package game-repo
	pull_package science-repo
	pull_package bash
	pull_package busybox
	pull_package command-not-found
	pull_package dash
	pull_package termux-tools
	pull_package termux-exec

	# Create bootstrap archive.
	create_bootstrap_archive "$package_arch"

	# Delete temporary directory once finished.
	rm -rf "$BOOTSTRAP_TMPDIR"
done
