ARCH = arm64
LAMBDA_FUNCTION = robochick
ZIP_FILE = bootrap.zip

all: test build_release

test:
	cargo test

build_release:
ifdef CONFIG_FILE_PATH
	@echo "Building lambda function for $(ARCH) in release mode..."
	cargo lambda build --release --$(ARCH) --output-format zip --include $(CONFIG_FILE_PATH)
else
	@echo "Missing CONFIG_FILE_PATH variable"
	@exit 1
endif

deploy: build_release
	@echo "Deploying lambda..."
	aws lambda update-function \
		--function-name $(LAMBDA_FUNCTION) \
		--zip-file fileb://$(ZIP_FILE)
		--region $(AWS_REGION)
