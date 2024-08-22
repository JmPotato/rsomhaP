# rsomhaP

> 'r{}'.format(''.join(sorted('[Pomash](https://github.com/JmPotato/Pomash)'))[::-1]) == 'rsomhaP'

## What is rsomhaP?

[Pomash](https://github.com/JmPotato/Pomash) is a blog engine written in Python, which was almost my first usable project created back in 2014. Although it hosted [my blog](https://ipotato.me) well for the past decade, since its code is somewhat messy and full of "young programmer" mistakes, I decided to rewrite it in Rust to make it more maintainable and as a commemorative project to its former self. Then here it is: rsomhaP.

rsomhaP is still a simple ready-to-use blog engine inheriting a lot from its predecessor:

- Markdown friendly.
- Monolithic web application without frontend and backend separation.
- The same concise and readable HTML and CSS styles.

But also introduces some new features:

- More secure admin authentication.
- Use any MySQL-compatible database to store your blog data.
- Easy to deploy with a simple [Dockerfile](./Dockerfile).

## Deployment

Edit your [`config.toml`](./config.toml) to your liking, then run:

```sh
cargo run --release
```

Or build a Docker image and run it:

```sh
docker build -t rsomhap .
docker run -p 5299:5299 rsomhap
```

Technically, you can deploy rsomhaP with modern SaaS infrastructures entirely free from scratch. For example:

- Use [TiDB Serverless](https://www.pingcap.com/tidb-serverless) as the MySQL-compatible database.
- Use [fly.io](https://fly.io) as the hosting service.

## License

[MIT](./LICENSE)
