ARCH := "arm64"
LAMBDA_FUNC := "robochick"
ZIP_FILE := "bootstrap.zip"
REGION := "eu-west-2"

test:
    cargo test

correct PATH:
	@test {{ env("AWS_PROFILE") }}
	cargo build --release -p lifejacket
	./target/release/lifejacket --config-path {{PATH}} --group "/aws/lambda/{{LAMBDA_FUNC}}"

publish PATH S3_DEST: (correct PATH)
	@test {{ env("AWS_PROFILE") }}
	@echo "Publishing corrected config file to {{ S3_DEST }}"
	aws s3 cp PATH s3://{{ S3_DEST }} --region {{ REGION }}

build_release CONFIG_FILE_PATH:
    @echo "Building lambda function for {{ARCH}} in release mode..."
    cargo lambda build --package robochick-events \
    	--release --{{ ARCH }} --output-format zip \
    	--include {{ CONFIG_FILE_PATH }}

deploy CONFIG_FILE_PATH: (build_release CONFIG_FILE_PATH)
	@echo "Deploying lambda..."
	aws lambda update-function \
		--function-name {{ LAMBDA_FUNC }} \
		--zip-file fileb://{{ ZIP_FILE }} \
		--region ${AWS_REGION} > /dev/null

