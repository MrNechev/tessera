# Tessera as a self-hosted web app (Docker)

The same Tessera you install on a desktop also runs as a server: open it in a
browser from any computer on your network. Everything works there — collages,
convert / compress / batch, the media library, documents, the canvas and
presentations — with your photo folders read straight from the server's disks.

| | Desktop app | Web app (Docker) |
| --- | --- | --- |
| Your photos | any folder on your computer | folders mounted into the container (*libraries*) |
| Results | saved where you choose | saved to the **Outputs** folder on the server, and downloaded to your computer if you like |
| Files from your computer | just pick or drop them | dropped/picked files are **uploaded** (files already in a library are found and used without uploading) |
| Login | none | one password |
| Video | WebKitGTK (see the README's *Known limitations*) | your browser (Chromium/Firefox): no limits |

## Quick start

```bash
mkdir ~/tessera && cd ~/tessera
curl -LO https://github.com/MrNechev/tessera/releases/latest/download/docker-compose.yml
curl -L -o .env https://github.com/MrNechev/tessera/releases/latest/download/env.example
nano .env          # set TESSERA_PASSWORD and LIBRARY_1_PATH at least
docker compose up -d
```

Open `http://<server>:8470` (or `https://<server>:8471`, see [HTTPS](#https-and-http2)).

The image is `ghcr.io/mrnechev/tessera:<version>` (`latest` = newest release).
To follow development instead, set `TESSERA_VERSION=edge` in `.env`: `edge` is
rebuilt from `main` after every push (Settings shows its build, e.g.
`edge-1a2b3c4`; *Check now* only knows releases). `sha-<commit>` tags pin one
build.
To build it yourself instead: clone the repository, set `TESSERA_SRC` in `.env`
to the checkout and run `docker compose build`.

## Folders

| In `.env` | In the container | What it holds |
| --- | --- | --- |
| `DATA_DIR` | `/data` | documents and their history, collage projects, settings, the library index, the HTTPS certificate. **Back this up.** |
| `CACHE_DIR` | `/cache` | thumbnails, video proxies, temporary uploads. Put it on an SSD. Safe to delete. |
| `OUTPUTS_DIR` | `/outputs` | everything Tessera makes. Shown in the Library as **Outputs**. |
| `INBOX_DIR` | `/inbox` | uploads you chose to keep. Shown in the Library as **Inbox**, usable in documents. |
| `LIBRARY_n_PATH` | `/libraries/<id>` | your photos and videos (up to four libraries in the compose file; add more lines if you need them). |

The container runs as `PUID:PGID` (default `1000:1000`): that user must be able
to read the libraries and write the other folders.

**Library IDs** (`LIBRARY_n_ID`) are stored in documents with every photo or
video they use. Choose them once and keep them.

> [!TIP]
> If your photos live in Nextcloud, mount the user's `files` folder as a library,
> but keep `OUTPUTS_DIR` and `INBOX_DIR` **outside** Nextcloud's data folder:
> files written there directly are invisible to Nextcloud until `occ files:scan` runs.

## Read-only and read-write libraries

Libraries are **read-only** by default: Tessera reads them for the library,
documents and tools, and saves results to Outputs. To let the tools write into
a library (convert "next to the originals", replace originals, move files
after renaming), do **both**:

1. mount it writable: `LIBRARY_2_MODE=rw`
2. list its id: `WRITABLE_LIBRARIES=phone` (comma-separated)

A library that is only one of the two stays read-only, and Tessera logs a
warning at start-up. Documents, the canvas and presentations never write to
libraries in any case: they only store references.

## Files from your computer

- **Pick** — every "Add photos" / "Choose folder" opens a browser of the server's
  libraries, Outputs and Inbox, with **Upload from this computer…** at the bottom.
- **Drop** files onto the window. Tessera first looks for each file in the
  libraries (same name and size); found files are used in place, the rest are uploaded.
- Uploads go to a temporary folder in `CACHE_DIR` and are deleted after a day,
  unless you tick **Keep in Inbox**. In a document, files that aren't on the
  server yet can be uploaded to the Inbox in one click.
- Uploads are sent in 8 MB chunks and resume after a network hiccup. Limit the
  size of one upload with `TESSERA_MAX_UPLOAD_MB`.

## Results

Saving a collage or running a tool writes the result to the server
(Outputs by default; any read-write library is allowed too) and, unless you turn
it off in Settings, **downloads** it to your computer. Many results download as
one zip. The Results card has a **Download** button as well.

## HTTPS and HTTP/2

Port 8471 serves HTTPS with HTTP/2 (much faster for pages with many videos and
thumbnails). Tessera creates its own private certificate authority on first
start (in `DATA_DIR/tls`) and a certificate for every name in
`TESSERA_TLS_HOSTS`. To get rid of the browser warning, import the CA once per
browser: open **System** in Tessera and follow *Trust the Tessera certificate*
(download `/ca.crt`, then import it as an authority and trust it for websites).
Or use your own certificate: `TESSERA_TLS_CERT` / `TESSERA_TLS_KEY`.

## GPU (VAAPI)

Videos your browser can't play (HEVC phone clips, MKV/AVI…) and very large
ones get H.264 proxies made with ffmpeg. With `/dev/dri` passed into the
container (default in the compose file), Intel and AMD GPUs do this work. The
**System** page shows whether the GPU is used. Without a GPU, set
`TESSERA_HWACCEL=none` and remove the `devices` section.

## Day to day

```bash
docker compose logs -f tessera     # logs
docker compose pull && docker compose up -d   # update to the newest image
```

**Backup:** `DATA_DIR` holds everything that matters. The Documents page also
has **Backup**, which downloads every document (with history) as a zip.

## Coming from Letopis

Tessera includes everything Letopis did and reads Letopis's data as it is:

1. Stop Letopis (`docker compose down` in its folder).
2. Copy its `data` folder to Tessera's `DATA_DIR` (and, to avoid re-making
   thumbnails, its `cache` folder to `CACHE_DIR`). `letopis.db` is renamed to
   `tessera.db` on first start; documents, history, preferences, the session
   secret and the certificate authority are reused, so browsers that trusted
   Letopis's certificate trust Tessera's (you sign in once more).
3. Keep the same library IDs (`LIBRARY_n_ID`): documents reference them.
4. Every `LETOPIS_*` setting still works; the new names are `TESSERA_*`.

## All settings

See [`packaging/docker/.env.example`](../packaging/docker/.env.example): every
setting with its default and an explanation.
