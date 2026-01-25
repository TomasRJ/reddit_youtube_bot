use std::{sync::Arc, time::Duration};

use chrono::Utc;
use sqlx::{Pool, Sqlite, query, query_scalar};
use tokio::sync::mpsc::{self, Receiver};
use tokio_stream::StreamExt;
use tokio_util::time::DelayQueue;

use crate::{
    infrastructure::AppState,
    server::{ApiError, SubCommand, subscribe_to_channel},
};

pub async fn handle_scheduler(
    state: &Arc<AppState>,
    receiver: Receiver<SubCommand>,
) -> Result<(), ApiError> {
    tokio::spawn(run_subscription_worker(state.db_pool.clone(), receiver));

    let with_expiration_exists = query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT
                s.channel_id
            FROM
                subscriptions s
            WHERE
                s.expires IS NOT NULL
            LIMIT 1
        ) AS "result: bool";        
        "#,
    )
    .fetch_one(&state.db_pool)
    .await?;

    if !with_expiration_exists {
        return Ok(());
    }

    let subscriptions_with_expiration = query!(
        r#"
        SELECT
            s.id,
            s.expires as "expires_at!: i64"
        FROM
            subscriptions s
        WHERE
            s.expires IS NOT NULL;
        "#,
    )
    .fetch_all(&state.db_pool)
    .await?;

    let now = Utc::now().timestamp();

    for subscription in subscriptions_with_expiration {
        let buffer = 3600; // 1 hour in seconds to resubscribe early
        let wait_secs = (subscription.expires_at - now - buffer).max(0);

        let _ = state
            .scheduler_sender
            .send(SubCommand::Schedule {
                subscription_id: subscription.id,
                wait_secs,
            })
            .await;
    }

    Ok(())
}

pub async fn run_subscription_worker(pool: Pool<Sqlite>, mut receiver: mpsc::Receiver<SubCommand>) {
    let mut queue = DelayQueue::new();
    println!("Subscription worker started.");

    loop {
        tokio::select! {
            // Handles scheduling for new subscriptions with expiration
            Some(cmd) = receiver.recv() => {
                match cmd {
                    SubCommand::Schedule { subscription_id, wait_secs } => {
                        println!("Now scheduling for subscription: {}", subscription_id);
                        queue.insert(subscription_id, Duration::from_secs(wait_secs as u64));
                    }
                }
            }
            // Handles subscription expirations
            Some(expired) = queue.next() => {
                let subscription_id = expired.into_inner();
                println!("Executing resubscribe for: {}", subscription_id);

                if let Err(e) = subscribe_to_channel_via_subscription_id(&pool, &subscription_id).await {
                    eprintln!("Resubscribe error for {}: {:?}", subscription_id, e);
                }
            }
        }
    }
}

async fn subscribe_to_channel_via_subscription_id(
    pool: &Pool<Sqlite>,
    subscription_id: &String,
) -> Result<(), ApiError> {
    let subscription = query!(
        r#"
        SELECT
            s.callback_url,
            s.channel_id,
            s.hmac_secret
        FROM
            subscriptions s
        WHERE
            s.id = ?;
        "#,
        subscription_id
    )
    .fetch_optional(&*pool)
    .await?;

    let subscription = if let Some(info) = subscription {
        info
    } else {
        return Err(ApiError::InternalError(format!(
            "No subscription for for id: {}",
            subscription_id,
        )));
    };

    subscribe_to_channel(
        &subscription.callback_url,
        &subscription.channel_id,
        &subscription.hmac_secret,
    )
    .await?;

    Ok(())
}
