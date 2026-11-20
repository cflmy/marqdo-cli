# Local development credentials

Credentials are **not committed**. They live in the git-ignored `.secrets/`
directory (or the untracked `.env.local`) so local development can use the
private Git and Woodpecker instances without rotating secrets through the
repository.

## Where to put them

`.secrets/dev.env` (ignored, mode 600) should define:

```dotenv
MARQDO_GIT_REMOTE=https://git.cflmy.cn/cflmy/marqdo-cli.git
MARQDO_GIT_USERNAME=<username>
MARQDO_GIT_TOKEN=<git access token>
MARQDO_WOODPECKER_URL=https://ci.cflmy.cn
MARQDO_WOODPECKER_TOKEN=<Woodpecker API token>
MARQDO_CI_SECRET=<optional CI secret>
```

Do not put real values in this document, tests, CI YAML or commit messages.
The repository's `.gitignore` excludes `.secrets/` and `.env.local`.

## Git push

` .secrets/git-askpass.sh ` returns the username/token for HTTPS prompts:

```bash
set -a; . ./.secrets/dev.env; set +a
GIT_ASKPASS="$PWD/.secrets/git-askpass.sh" git push origin master
```

Never embed the token in a remote URL stored in `.git/config`, shell history or
a CI file.

## Woodpecker API

```bash
set -a; . ./.secrets/dev.env; set +a
curl -fsS -H "Authorization: Bearer $MARQDO_WOODPECKER_TOKEN" \
  "$MARQDO_WOODPECKER_URL/api/user/repos"
```

CI itself is defined in `.woodpecker.yml`; the token is only for local API
queries/triggering, not for the pipeline. Configure any privileged secrets in
the Woodpecker repository settings, never in this repository.

## What is tracked

- `.woodpecker.yml` — public CI pipeline definition.
- this document — names/usage only, no values.
- `.secrets/` and `.env.local` — ignored local files.
