<script lang="ts">
  import { onMount } from 'svelte';
  import { api, fmtBytes } from '../lib/api';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import Icon from './Icon.svelte';

  let { debug, ontoggledebug }: { debug: boolean; ontoggledebug: () => void } = $props();
  let s = $state<any>(null);
  let err = $state('');

  async function load() {
    try {
      s = await api.status();
      err = '';
    } catch (e) {
      err = (e as Error).message;
    }
  }

  // Can this browser decode video in hardware? mediaCapabilities reports "powerEfficient"
  // for hardware decoders (that is what keeps many videos smooth at once).
  const CODECS: [string, string, number, number][] = [
    ['H.264 1080p', 'video/mp4; codecs="avc1.640028"', 1920, 1080],
    ['H.264 4K', 'video/mp4; codecs="avc1.640033"', 3840, 2160],
    ['HEVC 4K', 'video/mp4; codecs="hvc1.1.6.L153.B0"', 3840, 2160],
    ['VP9 4K', 'video/webm; codecs="vp09.00.51.08"', 3840, 2160],
    ['AV1 4K', 'video/mp4; codecs="av01.0.12M.08"', 3840, 2160],
  ];
  let decode = $state<{ name: string; supported: boolean; smooth: boolean; hw: boolean }[] | null>(null);
  async function checkDecode() {
    const mc = (navigator as any).mediaCapabilities;
    if (!mc?.decodingInfo) {
      decode = [];
      return;
    }
    decode = await Promise.all(
      CODECS.map(async ([name, contentType, width, height]) => {
        try {
          const r = await mc.decodingInfo({ type: 'file', video: { contentType, width, height, bitrate: width > 2000 ? 40e6 : 8e6, framerate: 30 } });
          return { name, supported: r.supported, smooth: r.smooth, hw: r.powerEfficient };
        } catch {
          return { name, supported: false, smooth: false, hw: false };
        }
      }),
    );
  }

  onMount(() => {
    checkDecode();
    load();
    const t = setInterval(load, 2000);
    return () => clearInterval(t);
  });

  async function rescan() {
    await api.scan();
    toasts.show('Rescan started');
    load();
  }

  async function stress() {
    const d = await api.stressTest();
    router.go(`/doc/${d.id}`);
  }

  const httpsUrl = $derived(
    s?.config?.httpsAddr ? `https://${location.hostname}:${String(s.config.httpsAddr).split(':').pop()}` : '',
  );
  const pct = (x: number) => (x * 100).toFixed(0) + '%';
</script>

<div class="page">
  <header class="top">
    <h1>System</h1>
    <div class="grow"></div>
    <label class="cb"><input type="checkbox" checked={debug} onchange={ontoggledebug} /> Performance overlay <span class="kbd">Ctrl+Shift+D</span></label>
  </header>
  {#if err}<p class="err">{err}</p>{/if}
  {#if s}
    <main>
      <section class="card">
        <h2><Icon name="shield" /> Connection</h2>
        <p>
          This page uses <b>{s.request.proto}</b>{s.request.tls ? ' over HTTPS' : ' (plain HTTP)'}.
          {#if s.request.proto !== 'HTTP/2.0'}
            Browsers only use HTTP/2 over HTTPS; with HTTP/1.1 they open at most ~6 connections, so pages with many videos
            load slower.
            {#if httpsUrl}Open <a href={httpsUrl}>{httpsUrl}</a> instead.{/if}
          {:else}
            Media requests are multiplexed over one connection.
          {/if}
        </p>
        <details>
          <summary>Trust the Tessera certificate (no more warnings, enables caching)</summary>
          <ol>
            <li>Download <a href="/ca.crt">tessera-ca.crt</a> (a private certificate authority created for this server only).</li>
            <li>
              Vivaldi / Chrome: <code>Settings → Privacy and security → Security → Manage certificates</code> →
              <b>Authorities</b> → <b>Import</b> → choose the file → tick “Trust this certificate for identifying websites”.
              On Linux you can also run <code>certutil -d sql:$HOME/.pki/nssdb -A -t "C,," -n tessera -i tessera-ca.crt</code>.
            </li>
            <li>Firefox: <code>Settings → Privacy &amp; Security → Certificates → View Certificates → Authorities → Import</code>.</li>
            <li>Restart the browser and open {httpsUrl || 'the HTTPS address'}.</li>
          </ol>
          <p class="muted">
            Until the certificate is trusted, Chromium-based browsers do not keep anything in their disk cache for the site, so
            thumbnails are downloaded again on every visit.
          </p>
        </details>
        <details>
          <summary>Check hardware video decoding in the browser</summary>
          <p>
            Open <code>vivaldi://gpu</code> (or <code>chrome://gpu</code>) and look for <b>Video Decode: Hardware accelerated</b>.
            While a video plays, <code>vivaldi://media-internals</code> shows the decoder in use (<code>VaapiVideoDecoder</code>
            = GPU; <code>FFmpegVideoDecoder</code>/<code>VpxVideoDecoder</code> = CPU). On Linux, Chromium may need
            <code>--enable-features=AcceleratedVideoDecodeLinuxGL,VaapiVideoDecoder</code> (or the same features in
            <code>vivaldi://flags</code>). In Firefox, check <code>about:support</code> → Media → hardware decoding.
          </p>
        </details>
      </section>

      <section class="card">
        <h2><Icon name="video" /> This browser's video decoding</h2>
        {#if decode === null}<p class="muted">Checking…</p>
        {:else if !decode.length}<p class="muted">This browser can't report its decoders.</p>
        {:else}
          <table class="dec">
            <tbody>
              {#each decode as d (d.name)}
                <tr>
                  <td>{d.name}</td>
                  <td>{#if !d.supported}<span class="chip bad">not supported</span>{:else if d.hw}<span class="chip ok">hardware</span>{:else}<span class="chip warn">software</span>{/if}</td>
                  <td class="muted">{d.supported ? (d.smooth ? 'smooth' : 'may stutter') : 'Tessera converts these'}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if decode.some((d) => d.supported && !d.hw && d.name.startsWith('H.264'))}
            <p class="hint">H.264 is decoded in software here. In Vivaldi/Chrome on Linux, start the browser with
              <code>--enable-features=AcceleratedVideoDecodeLinuxGL,VaapiVideoDecoder</code> and check <code>vivaldi://gpu</code>.</p>
          {:else if decode.find((d) => d.name === 'H.264 1080p')?.hw}
            <p class="hint ok">Hardware decoding is active: many videos can play at once smoothly.</p>
          {/if}
        {/if}
      </section>

      <section class="card">
        <h2><Icon name="folder" /> Libraries</h2>
        <table>
          <thead><tr><th>Library</th><th>Files</th><th>Images</th><th>Videos</th><th>GIFs</th><th>Size</th></tr></thead>
          <tbody>
            {#each s.libraries as l (l.id)}
              <tr>
                <td>{l.name} <span class="muted">({l.id})</span></td><td>{l.counts.total.toLocaleString()}</td>
                <td>{(l.counts.byKind.image ?? 0).toLocaleString()}</td><td>{(l.counts.byKind.video ?? 0).toLocaleString()}</td>
                <td>{(l.counts.byKind.gif ?? 0).toLocaleString()}</td><td>{fmtBytes(l.counts.bytes)}</td>
              </tr>
            {/each}
            <tr class="total">
              <td>All</td><td>{s.counts.total.toLocaleString()}</td><td>{(s.counts.byKind.image ?? 0).toLocaleString()}</td>
              <td>{(s.counts.byKind.video ?? 0).toLocaleString()}</td><td>{(s.counts.byKind.gif ?? 0).toLocaleString()}</td><td>{fmtBytes(s.counts.bytes)}</td>
            </tr>
          </tbody>
        </table>
        <p>
          Scan: {#if s.scan.running}<b>running</b> ({s.scan.library}, {s.scan.seen.toLocaleString()} files seen){:else}last finished
            {s.scan.endedAt && !s.scan.endedAt.startsWith('0001') ? new Date(s.scan.endedAt).toLocaleString() : 'never'} · {s.scan.changed} new/changed · {s.scan.removed} removed{/if}
          {#if s.counts.unprobed} · reading metadata of {s.counts.unprobed.toLocaleString()} files{/if}
          {#if s.scan.error}<br /><span class="err">{s.scan.error}</span>{/if}
        </p>
        <p class="muted">Automatic rescan every {s.config.scanInterval}{s.config.watch ? ', plus a folder watcher for instant updates' : ''}.</p>
        <button class="btn" onclick={rescan} disabled={s.scan.running}><Icon name="refresh" /> Rescan now</button>
      </section>

      <section class="card">
        <h2><Icon name="cpu" /> Processing</h2>
        <p>
          GPU: <b>{s.hw.mode === 'vaapi' ? 'VAAPI' : 'off (CPU)'}</b> <span class="muted">— {s.hw.note}</span><br />
          Image thumbnails: <b>{s.vips ? 'libvips' : 'ffmpeg (libvips missing)'}</b> · {s.runtime.cpus} CPU threads · Go heap {s.runtime.heapMB} MB
        </p>
        <table>
          <thead><tr><th>Queue</th><th>Workers</th><th>Running</th><th>Waiting (on screen)</th><th>Waiting (background)</th><th>Done</th><th>Failed</th><th>Avg</th></tr></thead>
          <tbody>
            {#each s.pools as p (p.name)}
              <tr><td>{p.name}</td><td>{p.workers}</td><td>{p.running}</td><td>{p.queuedHigh}</td><td>{p.queuedLow}</td><td>{p.completed}</td><td>{p.failed}</td><td>{p.avgMs.toFixed(0)} ms</td></tr>
            {/each}
          </tbody>
        </table>
        {#if s.transcodes?.length}
          <h3>Transcoding now</h3>
          {#each s.transcodes as t (t.key)}
            <div class="tc">
              <span class="name">{t.file || t.key}</span>
              <div class="bar"><div style="width:{(t.progress * 100).toFixed(1)}%"></div></div>
              <span class="muted">{pct(t.progress)} · {t.seconds.toFixed(0)} s</span>
            </div>
          {/each}
        {/if}
      </section>

      <section class="card">
        <h2><Icon name="activity" /> Cache</h2>
        <p class="muted">{s.cache.dir}</p>
        <table>
          <thead><tr><th></th><th>Size</th><th>Served from cache</th><th>Generated on request</th><th>Hit rate</th></tr></thead>
          <tbody>
            <tr><td>Thumbnails</td><td>{fmtBytes(s.cache.bytes.thumbs)}</td><td>{s.cache.thumbs.hits}</td><td>{s.cache.thumbs.misses}</td><td>{pct(s.cache.thumbs.hitRate)}</td></tr>
            <tr><td>GIF videos</td><td>{fmtBytes(s.cache.bytes.gif)}</td><td>{s.cache.gif.hits}</td><td>{s.cache.gif.misses}</td><td>{pct(s.cache.gif.hitRate)}</td></tr>
            <tr><td>Video proxies</td><td>{fmtBytes(s.cache.bytes.proxy)}</td><td>{s.cache.proxy.hits}</td><td>{s.cache.proxy.misses}</td><td>{pct(s.cache.proxy.hitRate)}</td></tr>
          </tbody>
        </table>
        <p class="muted">Counts are server-side since start. Browsers also cache derived files forever (immutable URLs), so repeat views often never reach the server.</p>
      </section>

      <section class="card">
        <h2><Icon name="video" /> Stress test</h2>
        <p>Builds a story from ~40 random videos, 25 GIFs and 40 images of your library. Scroll it with the performance overlay on: active video elements should stay low and scrolling smooth.</p>
        <button class="btn" onclick={stress}>Create stress-test story</button>
      </section>

      <p class="muted ver">Tessera {s.version} · up {s.uptime} · playable codecs: {s.config.playableVideo.join(', ')}</p>
    </main>
  {/if}
</div>

<style>
  .page {
    height: 100%;
    overflow: auto;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 36px 44px 4px;
  }
  h1 {
    margin: 0;
    font-size: 30px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }
  .grow {
    flex: 1;
  }
  .cb {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  main {
    max-width: 1040px;
    padding: 20px 44px 60px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 16px 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 10px;
    font-size: 15px;
  }
  h3 {
    font-size: 13px;
    margin: 14px 0 6px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    margin: 6px 0 10px;
  }
  th,
  td {
    text-align: left;
    padding: 5px 8px;
    border-bottom: 1px solid var(--line);
  }
  th {
    color: var(--text-3);
    font-weight: 500;
  }
  .total td {
    font-weight: 600;
  }
  details {
    margin: 8px 0;
  }
  summary {
    cursor: pointer;
    color: var(--blue);
  }
  code {
    font: 12px var(--mono);
    background: var(--surface-3);
    padding: 1px 5px;
    border-radius: 4px;
  }
  .err {
    color: var(--red);
  }
  .tc {
    display: grid;
    grid-template-columns: 1fr 200px 110px;
    gap: 10px;
    align-items: center;
    font-size: 12.5px;
    margin: 4px 0;
  }
  .tc .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .bar {
    height: 6px;
    background: var(--surface-3);
    border-radius: 3px;
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
  }
  .ver {
    text-align: center;
    font-size: 12px;
  }
  .dec {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .dec td {
    padding: 5px 4px;
    border-bottom: 1px solid var(--line);
  }
  .chip.ok {
    background: #73c99133;
    color: var(--green);
  }
  .hint {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .hint.ok {
    color: var(--green);
  }
</style>
