use sqlx::{Pool, Sqlite, query, query_as};

use crate::server::ApiError;

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
    pub expires: i64,
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
            "No subscription found for channel id: {}",
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
