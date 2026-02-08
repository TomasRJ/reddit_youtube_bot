# Reddit YouTube bot

This bot will automatically post a Reddit post when a new video is uploaded to a YouTube channel.
It current has 2 HTML forms:

1. Add a [Reddit App](https://reddit.com/prefs/apps/) to the service.
2. Subscribe to a Youtube channel via [PubSubHubbub](https://pubsubhubbub.appspot.com/), a server-to-server publish/subscribe protocol supported by YouTube/Google.

## Requirements

- [Git](https://git-scm.com/install/)
- [Rust](https://rust-lang.org/tools/install/)

## How to run

1. Clone the repo to a desired location:
   - `git clone https://github.com/TomasRJ/reddit_youtube_bot.git`
2. Install [SQLx CLI](https://crates.io/crates/sqlx-cli)
3. In the project dir run:
   1. `sqlx database create`
   2. `sqlx migration run`
4. Create a `.env` in the project dir with the following values:

    ```plaintext
    DATABASE_URL=sqlite://db.sqlite
    CLIENT_ID=SOME_ID
    CLIENT_SECRET=SOME_SECRET
    BASE_URL=http://localhost:3000
    ```

5. Run `cargo run start`
   1. You can use a custom port with: `cargo run start --port PORT`
   2. This project uses [bacon](https://dystroy.org/bacon/#installation) to make changes i development hot-reloadable. To use it in this project run it with `bacon webserver` in the project dir.
6. Go to <http://localhost:PORT> to view the frontend or to <http://localhost:PORT/rapidoc> to view the project's OpenAPI documentation (via [rapidoc](https://rapidocweb.com/))
