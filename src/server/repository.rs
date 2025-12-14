use sqlx::{Pool, Sqlite, query_as};

use crate::server::ApiError;

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        ApiError::InternalError(format!("SQL query error: {}", error))
    }
}

pub struct Subscription {
    pub channel_id: String,
    pub hmac_secret: String,
    pub expires: i64,
    pub reddit_account_id: i64,
    pub post_shorts: bool,
}

pub async fn get_subscription_for_user(
    pool: &Pool<Sqlite>,
    user_id: &i64,
    channel_id: &String,
) -> Result<Subscription, ApiError> {
    let subscription = query_as!(
        Subscription,
        r#"
        SELECT
            s.channel_id,
            s.hmac_secret,
            s.expires,
            s.reddit_account_id,
            s.post_shorts as "post_shorts: bool"
        FROM
            user_subscriptions us
        INNER JOIN subscriptions s ON
            us.channel_id = s.channel_id
        WHERE
            us.user_id = ?
            AND s.channel_id = ?;
        "#,
        user_id,
        channel_id
    )
    .fetch_optional(&*pool)
    .await?;

    match subscription {
        Some(sub) => Ok(sub),
        None => Err(ApiError::NotFound(format!(
            "No subscription found for channel id: {}",
            channel_id
        ))),
    }
}
