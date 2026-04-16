// Small display helpers used by the Dashboard.  Kept locale-aware to French
// strings matching the existing UI copy.

export function formatRelativeTime(isoString: string | null | undefined): string {
  if (!isoString) return 'Jamais';
  const then = new Date(isoString).getTime();
  if (Number.isNaN(then)) return 'Jamais';
  const diffSec = Math.floor((Date.now() - then) / 1000);
  if (diffSec < 60) return "À l'instant";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `Il y a ${diffMin}m`;
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return `Il y a ${diffHr}h`;
  const diffDay = Math.floor(diffHr / 24);
  return `Il y a ${diffDay}j`;
}

export function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '0 o';
  const units = ['o', 'Ko', 'Mo', 'Go', 'To'];
  const idx = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const val = bytes / Math.pow(1024, idx);
  return `${idx === 0 ? val : val.toFixed(1)} ${units[idx]}`;
}

/** Parse a size string like "5Go", "10GB", "500Mo" into bytes.  Returns 0 on
 *  invalid input.  Accepts French (Go/Mo/Ko/o) and English (GB/MB/KB/B) units
 *  since the backend round-trips strings that may come from legacy configs. */
export function parseSizeToBytes(input: string): number {
  const m = input.trim().toUpperCase().match(/^(\d+(?:\.\d+)?)\s*(GB|GO|MB|MO|KB|KO|B|O)?$/);
  if (!m || m[1] === undefined) return 0;
  const n = parseFloat(m[1]);
  if (Number.isNaN(n)) return 0;
  const unit = m[2] ?? 'GB';
  if (unit === 'GB' || unit === 'GO') return n * 1024 ** 3;
  if (unit === 'MB' || unit === 'MO') return n * 1024 ** 2;
  if (unit === 'KB' || unit === 'KO') return n * 1024;
  return n;
}

export function sanitizePath(name: string): string {
  return name.replace(/[/\\:*?"<>|]/g, '_').trim() || '_';
}

export function truncatePath(fullPath: string | null | undefined, maxLen = 40): string {
  if (!fullPath) return '';
  if (fullPath.length <= maxLen) return fullPath;
  const parts = fullPath.split('/').filter(Boolean);
  if (parts.length <= 2) return fullPath;
  return `…/${parts.slice(-2).join('/')}`;
}
