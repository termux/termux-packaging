RUST_VERSION=1.43.1
IMAGE=termux/packaging

linux-static-executable:
	docker run --rm -v "$(PWD)":/build fredrikfornwall/rust-static-builder:$(RUST_VERSION)

docker-image: linux-static-executable
	docker build -t $(IMAGE) .

push-docker-image: docker-image
	docker push $(IMAGE)

.PHONY: docker-image push-docker-image
