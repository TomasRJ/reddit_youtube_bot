use std::{sync::Arc, time::Duration};

use sqlx::{Pool, Sqlite, query, query_scalar};
use tokio::sync::mpsc::Receiver;
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

    let subscriptions_exist = query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT
                s.id
            FROM
                subscriptions s
            LIMIT 1
        ) AS "result: bool";        
        "#,
    )
    .fetch_one(&state.db_pool)
    .await?;

    if !subscriptions_exist {
        return Ok(());
    }

    let subscriptions = query!(
        r#"
        SELECT
            s.id
        FROM
            subscriptions s;
        "#,
    )
    .fetch_all(&state.db_pool)
    .await?;

    for subscription in subscriptions {
        let _ = state
            .scheduler_sender
            .send(SubCommand::Schedule {
                subscription_id: subscription.id,
                wait_secs: 5,
            })
            .await;
    }

    Ok(())
}

pub async fn run_subscription_worker(pool: Pool<Sqlite>, mut receiver: Receiver<SubCommand>) {
    let mut queue = DelayQueue::new();
    println!("Subscription worker started.");

    loop {
        tokio::select! {
            // Handles scheduling for subscriptions with expiration
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
            "No subscription found for id: {}",
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
