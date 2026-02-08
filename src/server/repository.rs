use chrono::Utc;
use sqlx::{Pool, Sqlite, query, query_as, query_scalar};

use crate::server::{
    ApiError,
    shared::{
        RedditAccountDTO, RedditAuthorization, RedditOAuthToken, Subreddit, Verification,
        VerificationMode, YouTubeSubscription,
    },
};

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        ApiError::InternalError(format!("SQL query error: {}", error))
    }
}

pub struct Subscription {
    pub id: String,
    pub channel_id: String,
    pub channel_name: String,
    pub hmac_secret: String,
    pub expires: Option<i64>,
    pub post_shorts: bool,
}

pub async fn get_subscription_details(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
) -> Result<Option<Subscription>, ApiError> {
    let subscription = query_as!(
        Subscription,
        r#"
        SELECT
            s.id,
            s.channel_id,
            s.channel_name,
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

    Ok(subscription)
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

pub async fn save_reddit_account(
    pool: &Pool<Sqlite>,
    username: &String,
    oauth_token: &RedditOAuthToken,
    reddit_auth_form_data: &RedditAuthorization,
) -> Result<i64, ApiError> {
    let expires_at = Utc::now().timestamp() + &oauth_token.expires_in;
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
        expires_at,
    )
    .execute(&*pool)
    .await?;

    if save_reddit_oauth_token_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "save_form_data rows_affected error: {:?}",
            save_reddit_oauth_token_result
        )));
    }

    Ok(save_reddit_oauth_token_result.last_insert_rowid())
}

pub async fn handle_youtube_subscription(
    pool: &Pool<Sqlite>,
    uuid_str: &String,
    expires_at: &Option<i64>,
    channel_id: &String,
    channel_name: &String,
    verification: &Verification,
    subscription_form: &YouTubeSubscription,
) -> Result<(), ApiError> {
    match verification.mode {
        VerificationMode::Subscribe => {
            let save_youtube_subscription_result = query!(
                r#"
                INSERT INTO subscriptions(id, channel_id, channel_name, hmac_secret, callback_url, expires, post_shorts)
                VALUES (?, ?, ?, ?, ?, ?, ?);
                "#,
                uuid_str,
                channel_id,
                channel_name,
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

pub async fn update_youtube_subscription(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
    expires_at: &Option<i64>,
) -> Result<(), ApiError> {
    let update_youtube_subscription_result = query!(
        r#"
        UPDATE
            subscriptions
        SET
            expires = ?
        WHERE
            id = ?;
        "#,
        expires_at,
        subscription_id,
    )
    .execute(&*pool)
    .await?;

    if update_youtube_subscription_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "update_youtube_subscription error: {:?}",
            update_youtube_subscription_result
        )));
    }

    Ok(())
}

pub async fn fetch_reddit_accounts_for_subscription(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
) -> Result<Vec<RedditAccountDTO>, ApiError> {
    let subscription_has_reddit_account = query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT
                sra.reddit_account_id
            FROM
                subscription_reddit_accounts sra
            WHERE
                sra.subscription_id = ?
            LIMIT 1
        ) AS "result: bool";        
        "#,
        subscription_id
    )
    .fetch_one(&*pool)
    .await?;

    if !subscription_has_reddit_account {
        return Ok(vec![]);
    }

    let reddit_accounts = query_as!(
        RedditAccountDTO,
        r#"
        SELECT
            ra.id,
            ra.username,
            ra.client_id,
            ra.user_secret,
            ra.moderate_submissions as "moderate_submissions: bool",
            ra.oauth_token,
            ra.expires_at
        FROM
            reddit_accounts ra
        INNER JOIN subscription_reddit_accounts sra ON
            sra.reddit_account_id = ra.id
        WHERE
            sra.subscription_id = ?;
        "#,
        subscription_id
    )
    .fetch_all(&*pool)
    .await?;

    Ok(reddit_accounts)
}

pub async fn update_reddit_oauth_token(
    pool: &Pool<Sqlite>,
    reddit_account_id: &i64,
    oauth_token: &RedditOAuthToken,
) -> Result<(), ApiError> {
    let expires_at = Utc::now().timestamp() + oauth_token.expires_in;
    let oauth_token_json_str = serde_json::to_string(oauth_token)?;

    let update_reddit_oauth_token_result = query!(
        r#"
        UPDATE
            reddit_accounts
        SET
            oauth_token = ?,
            expires_at = ?
        WHERE
            id = ?;
        "#,
        oauth_token_json_str,
        expires_at,
        reddit_account_id,
    )
    .execute(&*pool)
    .await?;

    if update_reddit_oauth_token_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "update_reddit_oauth_token error: {:?}",
            update_reddit_oauth_token_result
        )));
    }

    Ok(())
}

pub async fn fetch_subreddits_for_reddit_account(
    pool: &Pool<Sqlite>,
    reddit_account_id: &i64,
) -> Result<Vec<Subreddit>, ApiError> {
    let reddit_account_has_subreddit = query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT
                ras.subreddit_id
            FROM
                reddit_account_subreddits ras
            WHERE
                ras.reddit_account_id = ?
            LIMIT 1
        ) AS "result: bool";        
        "#,
        reddit_account_id
    )
    .fetch_one(&*pool)
    .await?;

    if !reddit_account_has_subreddit {
        return Ok(vec![]);
    }

    let subreddits = query_as!(
        Subreddit,
        r#"
        SELECT
            s.id,
            s.name,
            s.title_prefix,
            s.title_suffix,
            s.flair_id
        FROM
            subreddits s
        INNER JOIN reddit_account_subreddits ras ON
            ras.subreddit_id = s.id
        WHERE
            ras.reddit_account_id = ?;
        "#,
        reddit_account_id
    )
    .fetch_all(&*pool)
    .await?;

    Ok(subreddits)
}

pub async fn video_already_submitted_to_subreddit(
    pool: &Pool<Sqlite>,
    subreddit_id: &i64,
    video_id: &String,
) -> Result<bool, ApiError> {
    let is_already_submitted = query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT
                s.id
            FROM
                submissions s
            WHERE
                s.subreddit_id = ?
                AND s.video_id = ?
            LIMIT 1
        ) AS "result: bool";        
        "#,
        subreddit_id,
        video_id
    )
    .fetch_one(&*pool)
    .await?;

    Ok(is_already_submitted)
}

pub async fn save_reddit_submission(
    pool: &Pool<Sqlite>,
    submission_id: &String,
    video_id: &String,
    reddit_account_id: &i64,
    subreddit_id: &i64,
    timestamp: &i64,
    stickied: &bool,
) -> Result<(), ApiError> {
    let save_reddit_submission_result = query!(
        r#"
        INSERT INTO submissions(id, video_id, stickied, subreddit_id, reddit_account_id, created_at)
        VALUES (?, ?, ?, ?, ?, ?);
        "#,
        submission_id,
        video_id,
        stickied,
        subreddit_id,
        reddit_account_id,
        timestamp,
    )
    .execute(&*pool)
    .await?;

    if save_reddit_submission_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "save_reddit_submission rows_affected error: {:?}",
            save_reddit_submission_result
        )));
    }

    Ok(())
}

pub async fn save_subscription_submission(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
    reddit_submission_id: &String,
) -> Result<(), ApiError> {
    let save_subscription_submission_result = query!(
        r#"
        INSERT INTO subscription_submissions(subscription_id, submission_id)
        VALUES (?, ?);
        "#,
        subscription_id,
        reddit_submission_id,
    )
    .execute(&*pool)
    .await?;

    if save_subscription_submission_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "save_subscription_submission rows_affected error: {:?}",
            save_subscription_submission_result
        )));
    }

    Ok(())
}

pub async fn fetch_subscriptions(pool: &Pool<Sqlite>) -> Result<Vec<Subscription>, ApiError> {
    let subscription = query_as!(
        Subscription,
        r#"
        SELECT
            s.id,
            s.channel_id,
            s.channel_name,
            s.hmac_secret,
            s.expires,
            s.post_shorts as "post_shorts: bool"
        FROM
            subscriptions s;
        "#,
    )
    .fetch_all(&*pool)
    .await?;

    Ok(subscription)
}

pub async fn fetch_reddit_accounts(pool: &Pool<Sqlite>) -> Result<Vec<RedditAccountDTO>, ApiError> {
    let subscription = query_as!(
        RedditAccountDTO,
        r#"
        SELECT
            ra.id,
            ra.username,
            ra.client_id,
            ra.user_secret,
            ra.moderate_submissions as "moderate_submissions: bool",
            ra.oauth_token,
            ra.expires_at
        FROM
            reddit_accounts ra;
        "#,
    )
    .fetch_all(&*pool)
    .await?;

    Ok(subscription)
}

pub struct RedditSubmission {
    pub id: String,
    pub stickied: bool,
}

pub async fn fetch_submissions_on_subreddit(
    pool: &Pool<Sqlite>,
    subreddit_id: i64,
) -> Result<Vec<RedditSubmission>, ApiError> {
    let submissions = query_as!(
        RedditSubmission,
        r#"
        SELECT
            s.id,
            s.stickied as "stickied: bool"
        FROM
            submissions s
        WHERE
            s.subreddit_id = ?
        ORDER BY
            s.created_at ASC;
        "#,
        subreddit_id
    )
    .fetch_all(&*pool)
    .await?;

    Ok(submissions)
}

pub async fn update_reddit_submission_sticky_state(
    pool: &Pool<Sqlite>,
    submission_id: &String,
    state: &bool,
) -> Result<(), ApiError> {
    let update_reddit_submission_result = query!(
        r#"
        UPDATE
            submissions
        SET
            stickied = ?
        WHERE
            id = ?;
        "#,
        state,
        submission_id,
    )
    .execute(&*pool)
    .await?;

    if update_reddit_submission_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "update_reddit_submission error: {:?}",
            update_reddit_submission_result
        )));
    }

    Ok(())
}

pub async fn get_or_create_subreddit(
    pool: &Pool<Sqlite>,
    subreddit_name: &String,
    flair_id: &Option<String>,
) -> Result<Subreddit, ApiError> {
    let subreddit = query_as!(
        Subreddit,
        r#"
        SELECT
            s.id,
            s.name,
            s.title_prefix,
            s.title_suffix,
            s.flair_id
        FROM
            subreddits s
        WHERE
            s.name = ?;
        "#,
        subreddit_name
    )
    .fetch_optional(&*pool)
    .await?;

    if let Some(sub) = subreddit {
        return Ok(sub);
    }

    let create_subreddit_result = query!(
        r#"
        INSERT INTO subreddits(name, flair_id)
        VALUES (?, ?);
        "#,
        subreddit_name,
        flair_id,
    )
    .execute(&*pool)
    .await?;

    if create_subreddit_result.rows_affected() != 1 {
        return Err(ApiError::InternalError(format!(
            "get_or_create_subreddit rows_affected error: {:?}",
            create_subreddit_result
        )));
    }

    let subreddit_id = create_subreddit_result.last_insert_rowid();

    let subreddit = query_as!(
        Subreddit,
        r#"
        SELECT
            s.id,
            s.name,
            s.title_prefix,
            s.title_suffix,
            s.flair_id
        FROM
            subreddits s
        WHERE
            s.id = ?;
        "#,
        subreddit_id
    )
    .fetch_one(&*pool)
    .await?;

    Ok(subreddit)
}
