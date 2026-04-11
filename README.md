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
- Use either MySQL or PostgreSQL to store your blog data.
- Easy to deploy with a single command or a simple [Dockerfile](https://github.com/JmPotato/rsomhaP/blob/main/Dockerfile).

## Deployment

Edit your [`config.toml`](https://github.com/JmPotato/rsomhaP/blob/main/config.toml) to your liking, then run:

When you use the split fields under `[database]`, set `backend = "mysql"` or `backend = "postgres"`.
If you provide `connection_url` in the config or `DATABASE_URL` in the environment, that full URL wins and its scheme still selects the driver:

- `mysql://...` for MySQL
- `postgres://...` / `postgresql://...` for PostgreSQL

The default sample config binds to `0.0.0.0` so Docker and public deployments work out of the box. If you want local-only access, change `[deploy].host` to `127.0.0.1`.

```sh
cargo run --release
```

Or build a Docker image and run it. The container still needs a reachable
database; the simplest path is to pass `DATABASE_URL` explicitly:

```sh
docker build -t rsomhap .
docker run -p 5299:5299 \
  -e DATABASE_URL='postgres://user:pass@host:5432/dbname' \
  rsomhap
```

If you prefer the split fields in `config.toml`, make sure the container can
actually reach that host. The default `127.0.0.1:4000` example works for a
local host process, but inside Docker `127.0.0.1` means the container itself.

Access the admin page at "http://{your-deployment-url}/admin" to manage your blog. The initial password is the same as the username configured in [`config.toml`](./config.toml), **which you should change after the first login as soon as possible.**.

Technically, you can deploy rsomhaP with modern SaaS infrastructures entirely free from scratch. For example:

- Use [TiDB Serverless](https://www.pingcap.com/tidb-serverless) for a MySQL deployment, or any managed PostgreSQL service for a Postgres deployment.
- Use [fly.io](https://fly.io) as the hosting service.
- Use [Cloudflare R2](https://www.cloudflare.com/developer-platform/r2) as the image hosting service.
- Use [WebP Cloud Services](https://webp.se) as the image proxy service.

## License

[MIT](https://github.com/JmPotato/rsomhaP/blob/main/LICENSE)
