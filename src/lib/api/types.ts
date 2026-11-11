/** Types shared with the Rust backend (serde camelCase). */

export type ImageFormat =
  | 'jpeg'
  | 'png'
  | 'webp'
  | 'avif'
  | 'tiff'
  | 'bmp'
  | 'gif'
  | 'ico'
  | 'qoi'
  | 'tga'
  | 'heic'
  | 'jxl'
  | 'raw';

export interface FormatCapabilities {
  format: ImageFormat;
  label: string;
  extension: string;
  canDecode: boolean;
  canEncode: boolean;
  lossy: boolean;
  lossless: boolean;
  alpha: boolean;
  exif: boolean;
  icc: boolean;
}

export type InstallKind = 'appimage' | 'flatpak' | 'deb' | 'rpm' | 'dev' | 'unknown';

export interface AppInfo {
  version: string;
  channel: string;
  platform: string;
  installKind: InstallKind;
  heif: boolean;
  formats: FormatCapabilities[];
  inputExtensions: string[];
  dataDir: string;
  cacheDir: string;
}

export type ThemePref = 'system' | 'dark' | 'light';
export type ExportFormat = 'jpeg' | 'png' | 'webp' | 'avif' | 'tiff' | 'pdf';

export interface Settings {
  saveDirectory: string;
  filenamePattern: string;
  exportFormat: ExportFormat;
  exportQuality: number;
  theme: ThemePref;
  checkUpdates: boolean;
  lastUpdateCheck: string | null;
  defaultPreset: string;
  defaultDpi: number;
  defaultOrientation: 'portrait' | 'landscape';
  recentImportDir: string | null;
  showWelcome: boolean;
}

export interface ImportedImage {
  id: string;
  path: string;
  name: string;
  width: number;
  height: number;
  format: ImageFormat;
  fileSize: number;
  takenAt: string | null;
  proxy: string;
  thumb: string;
}

export interface ImportFailure {
  path: string;
  error: string;
}

export interface ImportResult {
  images: ImportedImage[];
  failed: ImportFailure[];
}

export interface ProgressEvent {
  task: string;
  done: number;
  total: number;
  current?: string | null;
}

export interface ExportResult {
  path: string;
  bytes: number;
  width: number;
  height: number;
}

export interface ProjectSummary {
  id: string;
  name: string;
  modified: string;
  path: string;
  thumb: string | null;
  width: number;
  height: number;
}

export interface UpdateInfo {
  current: string;
  latest: string | null;
  hasUpdate: boolean;
  url: string | null;
  notes: string | null;
  publishedAt: string | null;
  installKind: InstallKind;
}

/* -------------------------------------------------- imaging (converter) */

export type MetadataMode = 'keepAll' | 'stripAll' | 'custom';

export interface MetadataPolicy {
  mode: MetadataMode;
  keepDateTime: boolean;
  keepCamera: boolean;
  keepExposure: boolean;
  keepLocation: boolean;
  keepAuthor: boolean;
  keepIcc: boolean;
  keepFileTimes: boolean;
}

export type ResizeMode =
  | { kind: 'none' }
  | { kind: 'percent'; percent: number }
  | { kind: 'fit'; width: number; height: number }
  | { kind: 'longEdge'; pixels: number };

export type TargetFormat = { kind: 'keep' } | { kind: 'to'; format: ImageFormat };
export type ConflictPolicy = 'rename' | 'overwrite' | 'skip';

export interface OutputSpec {
  directory: string | null;
  suffix: string;
  conflict: ConflictPolicy;
}

export type QualityMode = { kind: 'original' } | { kind: 'reduce'; percent: number } | { kind: 'fixed'; quality: number };

/** `srgb` converts wide-gamut photos (Display P3, Adobe RGB…) to sRGB; `keep` leaves pixels and profile untouched. */
export type ColorMode = 'srgb' | 'keep';

export interface ConvertOptions {
  format: TargetFormat;
  quality: QualityMode;
  resize: ResizeMode;
  metadata: MetadataPolicy;
  output: OutputSpec;
  background: [number, number, number];
  color: ColorMode;
}

export type CompressLevel =
  | { kind: 'lossless' }
  | { kind: 'visuallyLossless' }
  | { kind: 'balanced' }
  | { kind: 'small' }
  | { kind: 'custom'; quality: number }
  | { kind: 'targetSize'; bytes: number };

export interface CompressOptions {
  level: CompressLevel;
  format: TargetFormat;
  resize: ResizeMode;
  metadata: MetadataPolicy;
  output: OutputSpec;
  keepIfLarger: boolean;
  color: ColorMode;
}

export type JobStatus = 'done' | 'skipped' | 'failed';

export interface JobResult {
  input: string;
  output: string | null;
  status: JobStatus;
  inputBytes: number;
  outputBytes: number;
  width: number;
  height: number;
  format: ImageFormat | null;
  quality: number | null;
  ssim: number | null;
  message: string | null;
  warnings: string[];
}

export interface CompressPreviewInfo {
  previewPath: string;
  originalPreviewPath: string;
  format: ImageFormat;
  quality: number | null;
  ssim: number | null;
  originalBytes: number;
  bytes: number;
  width: number;
  height: number;
}

export interface MetadataField {
  group: string;
  ifd: string;
  tag: string;
  value: string;
}

export interface MetadataSummary {
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  software: string | null;
  dateTaken: string | null;
  dateDigitized: string | null;
  dateModified: string | null;
  exposureTime: string | null;
  fNumber: number | null;
  iso: number | null;
  focalLength: number | null;
  focalLength35mm: number | null;
  gpsLatitude: number | null;
  gpsLongitude: number | null;
  gpsAltitude: number | null;
  artist: string | null;
  copyright: string | null;
  description: string | null;
  orientation: number;
  hasExif: boolean;
  hasIcc: boolean;
  hasXmp: boolean;
  iccDescription: string | null;
  iccIsSrgb: boolean;
  width: number;
  height: number;
  format: ImageFormat | null;
  fileSize: number;
  fields: MetadataField[];
}

export function defaultMetadataPolicy(): MetadataPolicy {
  return {
    mode: 'keepAll',
    keepDateTime: true,
    keepCamera: true,
    keepExposure: true,
    keepLocation: true,
    keepAuthor: true,
    keepIcc: true,
    keepFileTimes: true,
  };
}

export interface ProbeInfo {
  path: string;
  format: ImageFormat;
  width: number;
  height: number;
  orientation: number;
  fileSize: number;
  takenAt: string | null;
}

export interface ProbeResult {
  files: ProbeInfo[];
  failed: ImportFailure[];
}
