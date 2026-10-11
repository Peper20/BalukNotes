# The storage server on a VPS

`notes-hub` keeps vault files for sync (architecture §9). It has no Typst and
fits a small machine. It listens on HTTP on localhost; HTTPS is done by the
reverse proxy. Commands that need root are marked `#`.

## 1. The binary

Two files in one folder, `/usr/local/bin`: `notes` is the thin command,
`notes-hub` the part (`notes --version` lists them). `--no-default-features`
leaves the device's HTTP client out. The VPS builds them itself, from the
sources sent by one command from the laptop: set it up once and update with
"Updating" below.

To do it by hand instead, build on a machine with Rust (the same or older
glibc than on the VPS) and copy the two files:

```sh
cargo build --release --locked -p notes -p notes-hub --no-default-features
scp target/release/notes target/release/notes-hub vps:/usr/local/bin/   # as root
```

## 2. A user and data

A separate system user, data in its home:

```sh
# useradd --system --create-home --home-dir /var/lib/baluk-notes --shell /usr/sbin/nologin baluk-notes
# sudo -u baluk-notes notes --data /var/lib/baluk-notes users add ivan     # asks for the password twice
```

`notes users list | passwd <login> | remove <login>` - the rest. Vault files
are in `/var/lib/baluk-notes/hub/<login>/<vault>/files/` (plain folders: back
up the whole data directory).

## 3. The service

`/etc/systemd/system/baluk-notes-hub.service`:

```ini
[Unit]
Description=baluk notes: storage server
After=network.target

[Service]
User=baluk-notes
ExecStart=/usr/local/bin/notes --data /var/lib/baluk-notes hub serve --addr 127.0.0.1:8422
Restart=on-failure
RestartSec=3
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/baluk-notes
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```sh
# systemctl daemon-reload && systemctl enable --now baluk-notes-hub
# journalctl -u baluk-notes-hub -f
```

## 4. nginx

The machine already serves a site with nginx: add a server block for a
subdomain (a DNS record first), next to the existing one, e.g.
`/etc/nginx/sites-available/notes` (or `conf.d/notes.conf`):

```nginx
server {
    listen 80;
    server_name notes.example.org;

    location / {
        proxy_pass http://127.0.0.1:8422;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_set_header X-Forwarded-For $remote_addr;
        # a file is sent whole (limit 64 MiB), a change request waits up to 30 s
        client_max_body_size 70m;
        proxy_read_timeout 60s;
        proxy_buffering off;
    }
}
```

```sh
# nginx -t && systemctl reload nginx
# certbot --nginx -d notes.example.org      # the certificate and the https block
```

The server must never be reachable past nginx: it trusts the `X-Forwarded-*`
headers. `--addr 127.0.0.1:...` guarantees that.

## 5. Devices

```sh
notes sync login notes.example.org --login ivan
notes sync link --vault "Notes"      # the first sync: uploads, downloads or merges
notes sync status
```

From then on the app (`notes app`, or `notes serve`) syncs linked vaults in
the background. Who wins when the same file changed on two devices - the
setting "Это устройство" -> `device.sync_prefer`.

A slip does not wipe the vault everywhere: if a round would delete a large
part of it (more than 10 files and over a fifth, or all of it) - because the
folder was emptied by hand, a disk was not mounted - it stops before it
deletes anything on the server. `notes sync status` and the settings say
"paused"; `notes sync confirm --vault "Notes"` (or the button in the settings)
deletes them, `notes sync restore --vault "Notes"` gets the files back from the
server. The same holds in the other direction, for files deleted on the
server.

## Updating

One command on the laptop; the VPS builds everything itself, the laptop
builds nothing and GitHub has no access to the server:

```sh
tools/deploy-hub.sh vps                 # the commit HEAD; --ref <git-ref> for another one
```

`vps` is an SSH host (`~/.ssh/config`) of the deploy user. The script
1. sends the sources of the commit with `git archive | ssh ... tar -x` into
   `~/baluk-notes/src` of the deploy user (uncommitted changes are not sent:
   the script says so); the build folder `~/baluk-notes/target` is kept, the
   next build is incremental;
2. builds there: `cargo build --release --locked -p notes -p notes-hub
   --no-default-features` at the lowest CPU and disk priority (`nice -n 19`,
   `ionice -c3`), after checking the Rust version against `rust-version` and
   at least 1.5 GB of free disk;
3. calls `sudo -n /usr/local/sbin/baluk-notes-hub-install`, which copies the
   two binaries to `/usr/local/bin` and restarts the service;
4. prints `notes --version` (the parts and their versions) and
   `systemctl is-active`.

A step that fails stops the script with the remote output. A failed build
leaves the running server untouched; an interrupted run is simply repeated
(the session sends keep-alive packets, so a long quiet build does not drop
it).

### One-time setup (as root, once)

The build needs a linker and rustup; no C library is compiled (`cargo tree`
for the two packages has no `-sys` crate except `libc`, no OpenSSL). Ubuntu
22.04:

```sh
# apt install gcc curl ca-certificates
```

Rust for the deploy user (a minimal profile: no docs, clippy or rustfmt). The
version must be at least the `rust-version` of the workspace; `stable` is:

```sh
$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
```

The helper (its source is in the repository: copy it, do not retype it) and
the sudoers rule. In the helper the user name is set by `sed`; the rule lets
that user run exactly this one script without a password:

```sh
# sed 's/^DEPLOY_USER=.*/DEPLOY_USER=<deploy-user>/' tools/server/baluk-notes-hub-install \
    | install -m 755 -o root -g root /dev/stdin /usr/local/sbin/baluk-notes-hub-install
# echo '<deploy-user> ALL=(root) NOPASSWD: /usr/local/sbin/baluk-notes-hub-install' > /etc/sudoers.d/baluk-notes-hub
# chmod 440 /etc/sudoers.d/baluk-notes-hub && visudo -cf /etc/sudoers.d/baluk-notes-hub
```

The helper takes no arguments: it copies only `notes` and `notes-hub` from
`/home/<deploy-user>/baluk-notes/target/release` (regular files, a path with
no symlinks) by a temporary name and a rename, so a running binary is
replaced safely. The service (section 3) must already exist.

### What the build costs on the VPS

Measured on the laptop for these two packages (release):

| | |
|---|---|
| peak memory, one job | ~460 MB (the biggest `rustc`; cargo and the rest ~50 MB) |
| peak memory, all cores | ~2.5 GB: with several cores pass `--jobs 1` (`tools/deploy-hub.sh vps --jobs 1`) |
| first build, one core | 2.5 min on a ~4.5 GHz core; on a 2.3 GHz shared core at lowest priority estimate 5-10 min |
| a change in our crates only | 30 s on the same core; estimate 1-2 min |
| `target/` | 264 MB (a release build has no debug info and no incremental data) |
| disk in total | ~1 GB: the minimal toolchain ~0.6 GB, the crates cache ~0.1 GB, `target/` 0.26 GB, sources ~30 MB |

A build with one job suits one core and fits in the memory of a small
machine with swap; on 1 GB RAM without swap add swap first. The times on the
VPS are an estimate, not a measurement. To reclaim disk, delete
`~/baluk-notes/target` (the next build is a full one).

### Trust

Whoever holds the laptop's SSH key can run any code on the VPS as the deploy
user, and through the helper replace the programs that run as `baluk-notes`.
The key is the only secret of this scheme; protect it (a passphrase). The
parts are matched by version number only (`notes --version`), not by a
signature or a hash.

## The browser version with Typst

On a machine that can build notes (a home server), the full app in a browser:
`notes serve --auth --addr 127.0.0.1:8421` behind the same kind of proxy
block; accounts - the same `notes users`. On a weak VPS it is not needed.
