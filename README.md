A library for retrying, with specific support for the Azure SDK for Rust (cf. https://github.com/Azure/azure-sdk-for-rust).

You can supply an option `Settings` parameter for controlling the maximum number of attempts, initial wait time, backoff and an optional rand generator.

# Example usage
```
use azure_data_cosmos::prelude::*;
use rand::prelude::*;
use retry_async::{retry, Error as RetryError, Settings as RetrySettings};
use std::{error::Error, time::Duration};
mod device;
mod user;
use user::User;

const COSMOS_ACCOUNT: &str = "XXX";
const COSMOS_MASTER_KEY: &str = "XXX";
const DATABASE_NAME: &str = "XXX";
const USER_COLLECTION: &str = "users";

#[derive(Debug)]
enum CustomError {
    NotFound,
    Other,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let authorization_token = AuthorizationToken::primary_from_base64(&COSMOS_MASTER_KEY)?;
    let client = CosmosClient::new(
        COSMOS_ACCOUNT.to_string(),
        authorization_token,
        CosmosOptions::default(),
    );
    let database = client.database_client(DATABASE_NAME);
    let collection = database.collection_client(USER_COLLECTION);

    let mut rng = rand::thread_rng();

    let mut s = RetrySettings {
        attempts: 5,
        initial_delay: Duration::from_millis(100),
        backoff: 2.0,
        rng: None, //Some(&mut rng),
    };

    // Get document.
    match retry(
        || async {
            println!("ALPHA");

            // return Err(RetryError::CustomTransient(CustomError::Other));

            let response: GetDocumentResponse<User> = collection
                .document_client::<String, String>(
                    "2ea7e0af-5864-4947-b13e-a786920864cb".to_string(),
                    &"2ea7e0af-5864-4947-b13e-a786920864cb".to_string(),
                )?
                .get_document()
                .into_future()
                .await?;

            match response {
                GetDocumentResponse::Found(response) => Ok(response.document.document),
                GetDocumentResponse::NotFound(_) => {
                    Err(RetryError::CustomPermanent(CustomError::NotFound))
                }
            }
        },
        None, // Some(&mut s),
    )
    .await
    {
        Ok(user) => {
            println!("BRAVO {}", user.id);
        }
        Err(err) => {
            println!("CHARLIE {:?}", err);
        }
    };

    if let Some(rng) = s.rng {
        let y: f64 = rng.gen();
        println!("ECHO {:?}", y);
    }

    let y: f64 = rng.gen();
    println!("DELTA {:?}", y);

    Ok(())
}
```