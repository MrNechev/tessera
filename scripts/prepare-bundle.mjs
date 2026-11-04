// Runs before Tauri bundles deb/rpm/AppImage: the generated desktop entry is
// called "Tessera.desktop", so point the AppStream launchable at it.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const src = join(root, 'packaging/linux/io.github.mrnechev.Tessera.metainfo.xml');
const outDir = join(root, 'src-tauri/gen/linux');
mkdirSync(outDir, { recursive: true });
const xml = readFileSync(src, 'utf8').replace(
  '<launchable type="desktop-id">io.github.mrnechev.Tessera.desktop</launchable>',
  '<launchable type="desktop-id">Tessera.desktop</launchable>',
);
writeFileSync(join(outDir, 'io.github.mrnechev.Tessera.metainfo.xml'), xml);
