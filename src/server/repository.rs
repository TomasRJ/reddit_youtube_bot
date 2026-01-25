use chrono::Utc;
use sqlx::{Pool, Sqlite, query, query_as, query_scalar};

use crate::server::{
    ApiError,
    shared::{
        RedditAuthorization, RedditOAuthToken, Verification, VerificationMode, YouTubeSubscription,
    },
};

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        ApiError::InternalError(format!("SQL query error: {}", error))
    }
}

#[allow(dead_code)]
pub struct Subscription {
    pub id: String,
    pub channel_id: String,
    pub hmac_secret: String,
    pub expires: Option<i64>,
    pub post_shorts: bool,
}

pub async fn get_subscription_details(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
) -> Result<Subscription, ApiError> {
    let subscription = query_as!(
        Subscription,
        r#"
        SELECT
            s.id,
            s.channel_id,
            s.hmac_secret,
            s.expires,
            s.post_shorts as "post_shorts: bool"
        FROM
            subscriptions s
        WHERE
            s.id = ?;
        "#,
        subscription_id
    )
    .fetch_optional(&*pool)
    .await?;

    match subscription {
        Some(sub) => Ok(sub),
        None => Err(ApiError::NotFound(format!(
            "No subscription found for subscription id: {}",
            subscription_id
        ))),
    }
}

pub async fn save_form_data(
    pool: &Pool<Sqlite>,
    key: &String,
    data: &String,
) -> Result<(), ApiError> {
    let save_form_data_result = query!(
        r#"
        INSERT INTO forms(id, form_data)
        VALUES (?, ?);
        "#,
        key,
        data
    )
    .execute(&*pool)
    .await?;

    if save_form_data_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "save_form_data rows_affected error: {:?}",
            save_form_data_result
        )));
    }

    Ok(())
}

pub async fn fetch_form_data<T>(pool: &Pool<Sqlite>, key: &String) -> Result<T, ApiError>
where
    T: serde::de::DeserializeOwned, // This allows T to be any struct
{
    let form_data_json = query_scalar!(
        r#"
        SELECT
            f.form_data
        FROM
            forms f
        WHERE
            f.id = ?;
        "#,
        key,
    )
    .fetch_optional(&*pool)
    .await?;

    match form_data_json {
        Some(json_string) => Ok(serde_json::from_str::<T>(&json_string)?),
        None => Err(ApiError::NotFound(format!(
            "No form data found for the state str: {}",
            key
        ))),
    }
}

pub async fn save_reddit_oauth_token(
    pool: &Pool<Sqlite>,
    username: &String,
    oauth_token: &RedditOAuthToken,
    reddit_auth_form_data: RedditAuthorization,
) -> Result<(), ApiError> {
    let expires_at_timestamp = Utc::now().timestamp() + &oauth_token.expires_in;
    let oauth_token_json_str = serde_json::to_string(&oauth_token)?;

    let save_reddit_oauth_token_result = query!(
        r#"
        INSERT INTO reddit_accounts(username, client_id, user_secret, oauth_token, expires_at)
        VALUES (?, ?, ?, ?, ?);
        "#,
        username,
        reddit_auth_form_data.client_id,
        reddit_auth_form_data.secret,
        oauth_token_json_str,
        expires_at_timestamp,
    )
    .execute(&*pool)
    .await?;

    if save_reddit_oauth_token_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "save_form_data rows_affected error: {:?}",
            save_reddit_oauth_token_result
        )));
    }

    Ok(())
}

pub async fn handle_youtube_subscription(
    pool: &Pool<Sqlite>,
    uuid_str: &String,
    expires_at: &Option<i64>,
    channel_id: &String,
    verification: &Verification,
    subscription_form: &YouTubeSubscription,
) -> Result<(), ApiError> {
    match verification.mode {
        VerificationMode::Subscribe => {
            let save_youtube_subscription_result = query!(
                r#"
                INSERT INTO subscriptions(id, channel_id, hmac_secret, callback_url, expires, post_shorts)
                VALUES (?, ?, ?, ?, ?, ?);
                "#,
                uuid_str,
                channel_id,
                subscription_form.hmac_secret,
                subscription_form.callback_url,
                expires_at,
                subscription_form.post_shorts,
            )
            .execute(&*pool)
            .await?;

            if save_youtube_subscription_result.rows_affected() != 1 {
                return Err(ApiError::InternalError(format!(
                    "save_form_data rows_affected error: {:?}",
                    save_youtube_subscription_result
                )));
            }

            Ok(())
        }
        VerificationMode::Unsubscribe => {
            query!(
                r#"
                DELETE FROM 
                    subscriptions
                WHERE
                    channel_id = ?;
                "#,
                channel_id
            )
            .execute(&*pool)
            .await?;

            Ok(())
        }
    }
}
