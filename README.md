## Git-backed Volume Driver for Docker.

Automatically clones a Git repository into the volume on mount and removes it on unmount.

### Installation

```bash
docker plugin install nerjs/gitvol
```

### Usage

Create a volume by specifying a repository URL and (optionally) a tag or branch:

```bash
docker volume create -d nerjs/gitvol \
  -o url=https://github.com/username/repository.git \
  -o tag=v1.0.0 \
  my-repo
```
Run a container with the volume:
```bash
docker run --rm -it -v my-repo:/data alpine ls -la /data
```
Remove the volume:
```bash
docker volume rm my-repo
```

---

### docker-compose / Swarm example

```yaml
version: "3.8"

services:
  app:
    image: alpine
    command: ["ls", "-la", "/data"]
    volumes:
      - my-repo:/data

volumes:
  my-repo:
    driver: nerjs/gitvol
    driver_opts:
      url: https://github.com/nerjs/gitvol.git
      # Choose ONE of the two lines below (or omit both). Tags are recommended.
      # tag: v1.0.0
      # branch: main
```

--- 

## Options

- `url` (required) — Git-compatible repository URL. See [Git URLs](https://git-scm.com/docs/git-clone#_git_urls).

- `tag` (optional) — checkout a specific tag (__recommended__).

- `branch` (optional) — checkout a branch. **Not recommended** since branch contents may change between mounts.

> `tag` and `branch` are **mutually exclusive**.

---

## Private repositories

Currently supported via embedding credentials into the URL, e.g.:

```bash
docker volume create -d nerjs/gitvol \
  -o url=https://<github_pat_token>@github.com/nerjs/gitvol.git \
  my-private-repo
```