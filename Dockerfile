FROM fredrikfornwall/scratch-with-certificates
COPY target/x86_64-unknown-linux-musl/release/termux-packaging /
ENTRYPOINT ["/termux-packaging"]
