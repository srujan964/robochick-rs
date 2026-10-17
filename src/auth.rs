use anyhow::anyhow;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_secretsmanager::{
    Client,
    error::SdkError,
    operation::{
        create_secret::{CreateSecretError, CreateSecretOutput},
        get_secret_value::{GetSecretValue, GetSecretValueError},
        update_secret::{UpdateSecretError, UpdateSecretOutput},
    },
};
use lambda_http::tracing;

pub async fn securely_store_oauth_tokens(
    token_response: String,
    secret_name: String,
) -> anyhow::Result<String> {
    let region = RegionProviderChain::default_provider().or_else("eu-west-2");
    let config = aws_config::from_env().region(region).load().await;
    let client = aws_sdk_secretsmanager::Client::new(&config);

    match client
        .get_secret_value()
        .secret_id(&secret_name)
        .send()
        .await
    {
        Ok(secret_val) => {
            tracing::info!(%secret_name,"Secret already exists. Attempting update");
            if update_existing_secret(&secret_name, token_response.as_ref(), &client)
                .await
                .is_ok()
            {
                tracing::info!(%secret_name, "Secret updated successfully.");
                return Ok(secret_name.to_string());
            }

            Err(anyhow!("Secret update failed"))
        }
        Err(e) => match e.into_service_error() {
            GetSecretValueError::ResourceNotFoundException(_) => {
                tracing::info!(%secret_name,"Secret doesn't exist, creating one");
                if create_new_secret(&secret_name, token_response.as_ref(), &client)
                    .await
                    .is_ok()
                {
                    tracing::info!(%secret_name,"Secret created successfully.");
                    return Ok(secret_name.to_string());
                }

                Err(anyhow!("Secret creation failed"))
            }
            other => {
                tracing::error!(
                    %secret_name,"Unknown error when checking if secret already exists"
                );
                Err(anyhow!(other))
            }
        },
    }
}

async fn update_existing_secret(name: &str, val: &str, client: &Client) -> anyhow::Result<()> {
    match client
        .update_secret()
        .secret_id(name)
        .secret_string(val)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::error!(err = %e, secret_name = %name, "Secret update failed");
            Err(anyhow!(e))
        }
    }
}

async fn create_new_secret(name: &str, val: &str, client: &Client) -> anyhow::Result<()> {
    match client
        .create_secret()
        .name(name)
        .secret_string(val)
        .send()
        .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::error!(err = %e, secret_name = %name, "Secret creation failed");
            Err(anyhow!(e))
        }
    }
}
