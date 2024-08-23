# rsomhaP

> 'r{}'.format(''.join(sorted('[Pomash](https://github.com/JmPotato/Pomash)'))[::-1]) == 'rsomhaP'

rsomhaP is a simple ready-to-use blog engine written in Rust.

[![Demo Deployment Status](https://github.com/JmPotato/rsomhaP/actions/workflows/fly-deploy.yml/badge.svg)](https://github.com/JmPotato/rsomhaP/actions/workflows/fly-deploy.yml)

## What is rsomhaP?

[Pomash](https://github.com/JmPotato/Pomash) is a blog engine written in Python, which was almost my first usable project created back in 2014. Although it hosted [my blog](https://ipotato.me) well for the past decade, since its code is somewhat messy and full of "young programmer" mistakes, I decided to rewrite it in Rust to make it more maintainable and as a commemorative project to its former self. Then here it is: rsomhaP.

rsomhaP is still a simple ready-to-use blog engine inheriting a lot from its predecessor:

- Markdown friendly.
- Monolithic web application without frontend and backend separation.
- Concise and readable HTML/CSS styles.

But also introduces some new features:

- More secure admin authentication.
- Use any MySQL-compatible database to store your blog data.
- Easy to deploy with a single command or a simple [Dockerfile](https://github.com/JmPotato/rsomhaP/blob/main/Dockerfile).

## Deployment

Edit your [`config.toml`](https://github.com/JmPotato/rsomhaP/blob/main/config.toml) to your liking, then run:

```sh
cargo run --release
```

Or build a Docker image and run it:

```sh
docker build -t rsomhap .
docker run -p 5299:5299 rsomhap
```

Access the admin page at "http://{your-deployment-url}/admin" to manage your blog. The initial password is the same as the username configured in [`config.toml`](https://github.com/JmPotato/rsomhaP/blob/62dd746dfd6f7413d161a1fde79b82a0589b241b/config.toml#L14), **which you should change after the first login as soon as possible.**.

Technically, you can deploy rsomhaP with modern SaaS infrastructures entirely free from scratch. For example:

- Use [TiDB Serverless](https://www.pingcap.com/tidb-serverless) as the MySQL-compatible database.
- Use [fly.io](https://fly.io) as the hosting service.
- Use [Cloudflare R2](https://www.cloudflare.com/developer-platform/r2) as the image hosting service.
- Use [WebP Cloud Services](https://webp.se) as the image proxy service.

## License

[MIT](https://github.com/JmPotato/rsomhaP/blob/main/LICENSE)
