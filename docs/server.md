# The storage server on a VPS

`notes-hub` keeps vault files for sync (architecture §9). It has no Typst and
fits a small machine. It listens on HTTP on localhost; HTTPS is done by the
reverse proxy. Commands that need root are marked `#`.

## 1. The binary

Build on a machine with Rust (the same or newer glibc on the VPS, or build
there) and copy two files to the VPS, into one folder:

```sh
cargo build --release -p notes -p notes-hub --no-default-features
scp target/release/notes target/release/notes-hub vps:/usr/local/bin/   # as root, or ~/.local/bin
```

`notes` is the thin command, `notes-hub` the part (`notes --version` lists
them). `--no-default-features` leaves the device's HTTP client out.

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

## The browser version with Typst

On a machine that can build notes (a home server), the full app in a browser:
`notes serve --auth --addr 127.0.0.1:8421` behind the same kind of proxy
block; accounts - the same `notes users`. On a weak VPS it is not needed.
