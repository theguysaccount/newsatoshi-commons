import { useState } from 'react';
import { Check, Copy } from 'lucide-react';

interface CopyButtonProps {
  value: string;
  label?: string;
  className?: string;
  size?: 'xs' | 'sm';
}

export function CopyButton({ value, label, className = '', size = 'xs' }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    if (!value) return;
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      const textarea = document.createElement('textarea');
      textarea.value = value;
      textarea.style.position = 'fixed';
      textarea.style.opacity = '0';
      document.body.appendChild(textarea);
      textarea.select();
      document.execCommand('copy');
      document.body.removeChild(textarea);
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      title={copied ? 'Copied' : `Copy ${label ?? 'value'}`}
      className={`inline-flex items-center gap-1 rounded border transition-colors cursor-pointer ${
        copied
          ? 'border-emerald-500/50 bg-emerald-500/15 text-emerald-300'
          : 'border-slate-700/80 bg-slate-900/80 text-slate-400 hover:border-slate-500 hover:text-slate-100'
      } ${size === 'xs' ? 'px-1.5 py-0.5 text-[10px]' : 'px-2.5 py-1 text-xs'} ${className}`}
    >
      {copied ? <Check className="h-3 w-3 shrink-0" /> : <Copy className="h-3 w-3 shrink-0" />}
      {label && <span className="font-medium">{copied ? 'Copied' : label}</span>}
    </button>
  );
}

interface HashPillProps {
  hash: string;
  head?: number;
  tail?: number;
  prefix?: string;
  className?: string;
  full?: boolean;
}

export function truncateHash(hash: string, head = 8, tail = 6): string {
  if (!hash) return '—';
  if (hash.length <= head + tail + 3) return hash;
  return `${hash.slice(0, head)}…${hash.slice(-tail)}`;
}

export function HashPill({
  hash,
  head = 8,
  tail = 6,
  prefix,
  className = '',
  full = false,
}: HashPillProps) {
  if (!hash) {
    return <span className="font-mono text-xs text-slate-500">—</span>;
  }
  const display = full ? hash : truncateHash(hash, head, tail);
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-md border border-slate-800/90 bg-slate-950/75 px-2 py-0.5 font-mono text-[11px] text-slate-300 ${className}`}
    >
      {prefix && <span className="text-slate-500 select-none">{prefix}</span>}
      <span className={full ? 'break-all' : 'whitespace-nowrap'} title={hash}>
        {display}
      </span>
      <CopyButton value={hash} className="border-0 bg-transparent px-0.5 py-0 hover:bg-slate-800/80" />
    </span>
  );
}
