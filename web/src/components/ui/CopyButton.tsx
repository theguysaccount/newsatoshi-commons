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
      className={`inline-flex items-center gap-1.5 rounded-lg border transition-colors cursor-pointer ${
        copied
          ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300'
          : 'border-white/[0.09] bg-white/[0.03] text-zinc-400 hover:border-white/[0.2] hover:text-zinc-100'
      } ${size === 'xs' ? 'px-2 py-1 text-[11px]' : 'px-3 py-1.5 text-xs'} ${className}`}
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
    return <span className="font-mono text-xs text-zinc-500">—</span>;
  }
  const display = full ? hash : truncateHash(hash, head, tail);
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-lg border border-white/[0.08] bg-[#0a0a0d] px-2.5 py-1 font-mono text-[11px] text-zinc-300 ${className}`}
    >
      {prefix && <span className="text-zinc-500 select-none">{prefix}</span>}
      <span className={full ? 'break-all' : 'whitespace-nowrap'} title={hash}>
        {display}
      </span>
      <CopyButton
        value={hash}
        className="border-0 bg-transparent px-0.5 py-0 hover:bg-white/[0.06]"
      />
    </span>
  );
}
