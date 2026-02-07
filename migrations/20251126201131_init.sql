CREATE TABLE users (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL
);

CREATE INDEX user_id_index ON users (id);

CREATE INDEX username_index ON users (username);

CREATE TABLE sessions (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX session_id_index ON sessions (id);

CREATE INDEX session_user_id_index ON sessions (user_id);

CREATE TABLE subscriptions (
    id TEXT NOT NULL PRIMARY KEY,
    channel_id TEXT NOT NULL,
    channel_name TEXT NOT NULL,
    hmac_secret TEXT NOT NULL,
    callback_url TEXT NOT NULL,
    expires INTEGER,
    post_shorts INTEGER NOT NULL
);

CREATE INDEX subscription_index ON subscriptions (channel_id);

CREATE TABLE user_subscriptions (
    user_id INTEGER NOT NULL,
    subscription_id TEXT NOT NULL,
    PRIMARY KEY (user_id, subscription_id),
    FOREIGN KEY (subscription_id) REFERENCES subscriptions(id) ON DELETE CASCADE
);

CREATE TABLE reddit_accounts (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL,
    client_id TEXT NOT NULL,
    user_secret TEXT NOT NULL,
    moderate_submissions INTEGER NOT NULL,
    oauth_token TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE INDEX reddit_accounts_index ON reddit_accounts (id);

CREATE TABLE subscription_reddit_accounts (
    subscription_id TEXT NOT NULL,
    reddit_account_id INTEGER NOT NULL,
    PRIMARY KEY (subscription_id, reddit_account_id),
    FOREIGN KEY (reddit_account_id) REFERENCES reddit_accounts(id) ON DELETE CASCADE
);

CREATE TABLE subreddits (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    title_prefix TEXT,
    title_suffix TEXT,
    flair_id TEXT
);

CREATE INDEX subreddits_index ON subreddits (id);

CREATE TABLE reddit_account_subreddits (
    reddit_account_id INTEGER NOT NULL,
    subreddit_id INTEGER NOT NULL,
    PRIMARY KEY (reddit_account_id, subreddit_id),
    FOREIGN KEY (subreddit_id) REFERENCES subreddits(id) ON DELETE CASCADE
);

CREATE TABLE submissions (
    id TEXT NOT NULL PRIMARY KEY,
    video_id TEXT NOT NULL,
    stickied INTEGER NOT NULL DEFAULT 0,
    reddit_account_id INTEGER NOT NULL,
    subreddit_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (reddit_account_id) REFERENCES reddit_accounts(id) ON DELETE CASCADE,
    FOREIGN KEY (subreddit_id) REFERENCES subreddits(id) ON DELETE CASCADE
);

CREATE TABLE subscription_submissions (
    subscription_id TEXT NOT NULL,
    submission_id TEXT NOT NULL,
    PRIMARY KEY (subscription_id, submission_id),
    FOREIGN KEY (submission_id) REFERENCES submissions(id) ON DELETE CASCADE
);


CREATE TABLE forms (
    id TEXT NOT NULL PRIMARY KEY,
    form_data TEXT NOT NULL 
);
