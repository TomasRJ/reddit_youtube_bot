use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::Utc;
use hmac::{Hmac, Mac, digest::crypto_common};

use utoipa_axum::{router::OpenApiRouter, routes};

use crate::{
    infrastructure::AppState,
    server::{
        ApiError, SubCommand,
        reddit::{
            get_associated_reddit_accounts_for_subscription, moderate_submission,
            submit_video_to_subreddit,
        },
        repository::{
            fetch_form_data, fetch_subreddits_for_reddit_account, get_subscription_details,
            handle_youtube_subscription, save_reddit_submission, save_subscription_submission,
            update_youtube_subscription, video_already_submitted_to_subreddit,
        },
        shared::{
            Author, Feed, HTTP_CLIENT, Verification, VerificationMode, YouTubeSubscription,
            extract_channel_id_from_topic_url,
        },
    },
};

pub fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::new()
        .routes(routes!(new_video_published))
        .routes(routes!(subscription_verification))
}

impl From<quick_xml::DeError> for ApiError {
    fn from(error: quick_xml::DeError) -> Self {
        ApiError::BadRequest(format!("Invalid XML request: {:?}", error))
    }
}

impl From<crypto_common::InvalidLength> for ApiError {
    fn from(error: crypto_common::InvalidLength) -> Self {
        ApiError::InternalError(format!("Error handling HMAC secret: {:?}", error))
    }
}

impl From<axum::http::header::ToStrError> for ApiError {
    fn from(error: axum::http::header::ToStrError) -> Self {
        ApiError::InternalError(format!(
            "The X-Hub-Signature header is not a valid string: {:?}",
            error
        ))
    }
}

type HmacSha1 = Hmac<sha1::Sha1>;

impl Feed {
    fn validate(hmac_secret: &String, headers: HeaderMap, body: String) -> Result<Feed, ApiError> {
        match headers.get("X-Hub-Signature") {
            Some(signature) => {
                let signature = if let Some(("sha1", hash)) = signature.to_str()?.split_once('=') {
                    hash
                } else {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid X-Hub-Signature header format: {:?}, expected sha1=signature",
                        signature
                    )));
                };

                if signature.len() != 40 {
                    return Err(ApiError::BadRequest(format!(
                        "Invalid SHA1 signature: {}, length in bytes: {}",
                        signature,
                        signature.len()
                    )));
                }

                let mut hasher = HmacSha1::new_from_slice(hmac_secret.as_bytes())?;
                hasher.update(body.as_bytes());
                let hash = hasher.finalize();

                let hash_string = format!("{:x}", hash.into_bytes()); // format the bytes to a lowercase hex string

                if signature.ne(&hash_string) {
                    return Err(ApiError::BadRequest(
                        "The signature in the header does not match the calculated signature"
                            .to_string(),
                    ));
                }

                let feed: Feed = quick_xml::de::from_str(&body)?;

                Ok(feed)
            }
            None => Err(ApiError::BadRequest(
                "The new video request has no X-Hub-Signature header.".to_string(),
            )),
        }
    }
}

/// New video published
#[utoipa::path(
        post,
        path = "/subscription/{id}",
        request_body(content = Feed, description = "Google PubSubHubbub XML request", content_type = "application/atom+xml"),
        description = "New video published request from Google PubSubHubbub",
        params(
            ("X-Hub-Signature" = String, Header, description = "Google PubSubHubbub HMAC signature for the request body in the form of \"sha1=signature\" where the signature is a 40-byte, hexadecimal representation of a SHA1 signature. Source https://pubsubhubbub.github.io/PubSubHubbub/pubsubhubbub-core-0.4.html#rfc.section.8", example = "sha1=e7667dbb6b9dc356ac8dd767560926d5403be497"),
            ("id" = String, Path, description = "Subscription id", example = "019ba504-70f5-7f35-9c2c-2f02b992af7e")
        ),
        responses(
            (status = 200, description = "Successful request."),
            (status = 400, description = "Bad request, possible malformed XML or X-Hub-Signature header."),
            (status = 404, description = "Subscription doesn't exists."),
        ),
        tag = "google"
    )]
#[axum::debug_handler]
async fn new_video_published(
    State(state): State<Arc<AppState>>,
    Path(subscription_id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<(), ApiError> {
    let subscription = get_subscription_details(&state.db_pool, &subscription_id)
        .await?
        .ok_or(ApiError::BadRequest(format!(
            "No subscription found for subscription id: {}",
            subscription_id
        )))?;

    let feed = Feed::validate(&subscription.hmac_secret, headers, body)?;
    println!(
        "Received video request (title: '{}' link: {}) published from '{}' (link: {})",
        feed.entry.title, feed.entry.link.href, feed.entry.author.name, feed.entry.author.uri
    );

    let published_diff = (feed.entry.updated - feed.entry.published).num_seconds();
    if published_diff > 60 {
        println!(
            "Video was determined to be an update to an old video, not a new video upload. The time difference between the 'updated' and 'published' fields was: {}",
            published_diff
        );
        return Ok(());
    }

    // Shorts are only posted when the user has explicitly set post_shorts to true.
    if feed.entry.link.href.contains("shorts") && !subscription.post_shorts {
        return Ok(());
    }

    let subscription_reddit_accounts =
        get_associated_reddit_accounts_for_subscription(&state, &subscription.id).await?;

    if subscription_reddit_accounts.is_empty() {
        println!(
            "The subscription: {} has no associated Reddit accounts to use for submit the video (title: '{}' link: {})",
            subscription_id, feed.entry.title, feed.entry.link.href
        );
        return Ok(());
    }

    println!(
        "Fetched {} associated reddit accounts for subscription: {}",
        subscription_reddit_accounts.len(),
        subscription.id
    );

    for reddit_account in subscription_reddit_accounts {
        let reddit_account_id = reddit_account.id;

        let reddit_account_subreddits =
            fetch_subreddits_for_reddit_account(&state.db_pool, &reddit_account_id).await?;

        if reddit_account_subreddits.is_empty() {
            println!(
                "The reddit account: {} has no associated subreddits to submit the video (title: '{}' link: {})",
                reddit_account.username, feed.entry.title, feed.entry.link.href
            );
            continue;
        }

        println!(
            "Fetched {} associated subreddits for reddit account: https://www.reddit.com/user/{}",
            reddit_account_subreddits.len(),
            reddit_account.username
        );

        for subreddit in reddit_account_subreddits {
            if video_already_submitted_to_subreddit(
                &state.db_pool,
                &subreddit.id,
                &feed.entry.yt_video_id,
            )
            .await?
            {
                println!(
                    "The video (title: '{}' link: {}) has been already submitted to the https://reddit.com/r/{} subreddit.",
                    feed.entry.title, feed.entry.link.href, subreddit.name,
                );
                continue;
            }

            println!(
                "Now submitting the new video (title: '{}' link: {}) to the following subreddit: {}",
                feed.entry.title, feed.entry.link.href, subreddit.name
            );

            let reddit_submission =
                submit_video_to_subreddit(&reddit_account, &subreddit, &feed.entry).await?;

            println!(
                "Reddit submission successful. URL: {}",
                reddit_submission.url
            );

            save_reddit_submission(
                &state.db_pool,
                &reddit_submission.id,
                &feed.entry.yt_video_id,
                &reddit_account_id,
                &subreddit.id,
                &Utc::now().timestamp(),
                &false,
            )
            .await?;

            save_subscription_submission(&state.db_pool, &subscription_id, &reddit_submission.id)
                .await?;

            if reddit_account.moderate_submissions {
                moderate_submission(&state, &reddit_account, &subreddit).await?;
            }
        }
    }

    Ok(())
}

/// Hub verification request
#[utoipa::path(
        get,
        path = "/subscription/{id}",
        description = "Google PubSubHubbub subscription verification",
        params(
            ("id" = String, Path, description = "Subscription id", example = "019ba504-70f5-7f35-9c2c-2f02b992af7e"),
            ("hub.mode" = VerificationMode, Query, description = "The literal string \"subscribe\" or \"unsubscribe\", which matches the original request to the hub from the subscriber.", example = "subscribe"),
            ("hub.topic" = String, Query, description = "The topic URL given in the corresponding subscription request.", example = "https://www.youtube.com/xml/feeds/videos.xml?channel_id=UCBR8-60-B28hp2BmDPdntcQ"),
            ("hub.challenge" = String, Query, description = "A hub-generated, random string that MUST be echoed by the subscriber to verify the subscription.", example = "14828210609622910347"),
            ("hub.lease_seconds" = Option<u64>, Query, description = "The hub-determined number of seconds that the subscription will stay active before expiring, measured from the time the verification request was made from the hub to the subscriber. This parameter MAY be present for unsubscribe requests and MUST be ignored by subscribers during unsubscription.", example = 432000)
        ),
        responses(
            (status = 200, description = "The challenge string.", body = String),
            (status = 400, description = "Missing required query arguments."),            
        ),
        tag = "google"
    )]
#[axum::debug_handler]
async fn subscription_verification(
    State(state): State<Arc<AppState>>,
    Path(subscription_id): Path<String>,
    Query(verification): Query<Verification>,
) -> Result<String, ApiError> {
    let subscription = get_subscription_details(&state.db_pool, &subscription_id).await?;
    let expires_at = match verification.lease_seconds {
        Some(wait_secs) => {
            let buffer = 3600; // 1 hour in seconds to resubscribe early

            // schedule the resubscription
            let _ = state
                .scheduler_sender
                .send(SubCommand::Schedule {
                    subscription_id: subscription_id.clone(),
                    wait_secs: (wait_secs - buffer).max(5),
                })
                .await;

            Some(Utc::now().timestamp() + wait_secs)
        }
        None => None,
    };

    match subscription {
        Some(existing_sub) => {
            println!(
                "Received Google PubSubHubbub resubscription request for YouTube channel: https://www.youtube.com/channel/{}",
                &existing_sub.channel_id
            );

            update_youtube_subscription(&state.db_pool, &subscription_id, &expires_at).await?;
        }
        None => {
            let channel_id = extract_channel_id_from_topic_url(&verification.topic)?.to_string();

            println!(
                "Received Google PubSubHubbub subscription verification request for YouTube channel: https://www.youtube.com/channel/{}",
                &channel_id
            );

            let subscription_form: YouTubeSubscription =
                fetch_form_data(&state.db_pool, &subscription_id).await?;

            let subscription_data = fetch_subscription_data(&channel_id).await?;

            handle_youtube_subscription(
                &state.db_pool,
                &subscription_id,
                &expires_at,
                &channel_id,
                &subscription_data.author.name,
                &verification,
                &subscription_form,
            )
            .await?;

            println!("Google PubSubHubbub subscription verification request handled.");
        }
    }

    Ok(verification.challenge)
}

#[derive(serde::Deserialize)]
struct SubscriptionData {
    author: Author,
}

async fn fetch_subscription_data(channel_id: &String) -> Result<SubscriptionData, ApiError> {
    let client = &HTTP_CLIENT;

    let subscription_data = client
        .get(format!(
            "https://www.youtube.com/feeds/videos.xml?channel_id={}",
            channel_id
        ))
        .send()
        .await?
        .text()
        .await?;

    let data: SubscriptionData = quick_xml::de::from_str(&subscription_data)?;

    Ok(data)
}
