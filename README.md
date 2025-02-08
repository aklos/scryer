# next-forge

## Notes

- Add attribution for IP geo lookup:

```
[Your site name or product name] uses the IP2Location LITE database for <a href="https://lite.ip2location.com">IP geolocation</a>.
```

## Development

1. Start Postgres proxy: `./database/proxy.sh`
1. Start Redis proxy: `fly redis connect`
1. Start app dev: `pnpm dev:app`
1. Start agent-api: `source venv/bin/activate && cd apps/agent-api && python main.py`

**Production-grade Turborepo template for Next.js apps.**

<div>
  <img src="https://img.shields.io/npm/dy/next-forge" alt="" />
  <img src="https://img.shields.io/npm/v/next-forge" alt="" />
  <img src="https://img.shields.io/github/license/haydenbleasel/next-forge" alt="" />
</div>

[next-forge](https://github.com/haydenbleasel/next-forge) is a [Next.js](https://nextjs.org/) project boilerplate for modern web application. It is designed to be a comprehensive starting point for new apps, providing a solid, opinionated foundation with a minimal amount of configuration.

Clone the repo using:

```sh
npx next-forge@latest init [my-app]
```

Then read the [docs](https://docs.next-forge.com) for more information.

<a href="https://github.com/haydenbleasel/next-forge/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=haydenbleasel/next-forge" />
</a>

Made with [contrib.rocks](https://contrib.rocks).
