use futures::Future;
use rand::prelude::*;
use std::{cell::RefCell, thread, time::Duration};

thread_local!(static RNG: RefCell<rand::prelude::ThreadRng> = {
    RefCell::new(rand::thread_rng())
});

#[derive(Debug)]
pub enum Details {
    Duplicate,
    Throttled,
    NotFound,
    Unspecified,
}

#[derive(Debug)]
pub enum Error<K> {
    Transient(Details),
    Permanent(Details),
    Exhausted(Details),
    CustomTransient(K),
    CustomPermanent(K),
    CustomExhausted(K),
}

impl<K> From<azure_core::error::Error> for Error<K> {
    fn from(error: azure_core::error::Error) -> Self {
        match error.kind() {
            azure_core::error::ErrorKind::HttpResponse {
                status,
                error_code: _error_code,
            } => match status {
                404 => Error::Permanent(Details::NotFound),
                409 => Error::Permanent(Details::Duplicate),
                429 => Error::Transient(Details::Throttled),
                _ => Error::Permanent(Details::Unspecified),
            },
            azure_core::error::ErrorKind::Io => Error::Transient(Details::Unspecified),
            azure_core::error::ErrorKind::DataConversion => Error::Permanent(Details::Unspecified),
            azure_core::error::ErrorKind::Credential => Error::Permanent(Details::Unspecified),
            azure_core::error::ErrorKind::Other => Error::Transient(Details::Unspecified),
        }
    }
}

pub struct Settings<'a> {
    pub attempts: u8,

    pub initial_delay: Duration,

    pub backoff: f64,

    // In case caller wants to reuse a rand generator.
    pub rng: Option<&'a mut rand::prelude::ThreadRng>,
}

pub async fn retry<'a, F, K, E, Fut>(
    func: F,
    settings: Option<&mut Settings<'a>>,
) -> Result<K, Error<E>>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<K, Error<E>>>,
{
    let mut settings_holder;
    let settings = match settings {
        Some(settings) => settings,
        None => {
            settings_holder = Settings {
                attempts: 5,
                initial_delay: Duration::from_millis(100),
                backoff: 2.0,
                rng: None,
            };
            &mut settings_holder
        }
    };

    // NOTE: We start index at one for easier comparison with attempts.
    let mut attempt = 1;

    let mut wait = Duration::default();
    loop {
        match func().await {
            Ok(k) => return Ok(k),
            Err(err) => {
                if attempt == settings.attempts {
                    let err = match err {
                        Error::Transient(err) => Error::Exhausted(err),
                        Error::Permanent(err) | Error::Exhausted(err) => Error::Exhausted(err),
                        Error::CustomPermanent(err)
                        | Error::CustomExhausted(err)
                        | Error::CustomTransient(err) => Error::CustomExhausted(err),
                    };

                    return Err(err);
                }

                match err {
                    Error::Transient(_) | Error::CustomTransient(_) => {
                        wait = if attempt == 1 {
                            settings.initial_delay
                        } else {
                            wait.mul_f64(settings.backoff)
                        };

                        // Randomized exponential backoff, to avoid thundering herd problem.

                        let fuzz: f64 = match &mut settings.rng {
                            Some(rng) => rng.gen(),
                            None => RNG.with(|rng| rng.borrow_mut().gen::<f64>()),
                        };

                        // Adjust random factor.
                        let fuzz = fuzz / 2.0 + 0.75;

                        // Calculate time to wait, including random factor.
                        let wait = wait.mul_f64(fuzz);

                        thread::sleep(wait);

                        attempt += 1;
                        continue;
                    }
                    Error::Permanent(_)
                    | Error::Exhausted(_)
                    | Error::CustomPermanent(_)
                    | Error::CustomExhausted(_) => return Err(err),
                }
            }
        };
    }
}
