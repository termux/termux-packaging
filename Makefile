IMAGE=termux/packaging

linux-static-executable:
	docker run --rm -v "$(PWD)":/build fredrikfornwall/rust-static-builder:1.41.0

docker-image: linux-static-executable
	docker build -t $(IMAGE) .

push-docker-image: docker-image
	docker push $(IMAGE)

.PHONY: docker-image push-docker-image
