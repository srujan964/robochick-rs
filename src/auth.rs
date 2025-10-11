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

pub async fn securely_store_oauth_tokens(token_response: String) -> anyhow::Result<String> {
    let region = RegionProviderChain::default_provider().or_else("eu-west-2");
    let config = aws_config::from_env().region(region).load().await;
    let client = aws_sdk_secretsmanager::Client::new(&config);

    let name = "robochick_rs_twitch_oauth";

    match client.get_secret_value().secret_id(name).send().await {
        Ok(secret_val) => {
            println!("Secret already exists. Attempting update");
            if update_existing_secret(name, token_response.as_ref(), &client)
                .await
                .is_ok()
            {
                println!("Secret updated successfully.");
                return Ok(name.to_string());
            }

            Err(anyhow!("Secret update failed"))
        }
        Err(e) => match e.into_service_error() {
            GetSecretValueError::ResourceNotFoundException(_) => {
                println!("Secret doesn't existing, creating one");
                if create_new_secret(name, token_response.as_ref(), &client)
                    .await
                    .is_ok()
                {
                    println!("Secret created successfully.");
                    return Ok(name.to_string());
                }

                Err(anyhow!("Secret creation failed"))
            }
            other => {
                println!("Unknown error when checking if secret already exists");
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
            println!("Secret update failed: {e}");
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
            println!("Secret creation failed: {e}");
            Err(anyhow!(e))
        }
    }
}
