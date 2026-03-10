import type { Contract, ContractItem } from "../types";
import { contractPassed } from "../types";
import { Shield, MessageCircle, Ban } from "lucide-react";

function sectionStats(items: ContractItem[]): { total: number; passed: number; failed: number; unchecked: number } {
  let passed = 0, failed = 0, unchecked = 0;
  for (const item of items) {
    const p = contractPassed(item);
    if (p === true) passed++;
    else if (p === false) failed++;
    else unchecked++;
  }
  return { total: items.length, passed, failed, unchecked };
}

function accentColor(stats: { total: number; passed: number; failed: number; unchecked: number }): string {
  if (stats.total === 0) return "";
  if (stats.failed > 0) return "text-red-500";
  if (stats.unchecked > 0) return "text-zinc-400 dark:text-zinc-500";
  return "text-emerald-500";
}

function SectionPill({ icon: Icon, stats }: {
  icon: typeof Shield;
  stats: { total: number; passed: number; failed: number; unchecked: number };
}) {
  if (stats.total === 0) return null;
  const color = accentColor(stats);
  const checked = stats.passed + stats.failed;
  const label = checked > 0 ? `${stats.passed}/${stats.total}` : `${stats.total}`;
  return (
    <span className={`inline-flex items-center gap-0.5 ${color}`}>
      <Icon size={9} strokeWidth={2.5} />
      <span className="text-[8px] font-semibold leading-none tabular-nums">{label}</span>
    </span>
  );
}

export function ContractBadge({ contract }: { contract?: Contract }) {
  if (!contract) return null;
  const expectStats = sectionStats(contract.expect);
  const askStats = sectionStats(contract.ask);
  const neverStats = sectionStats(contract.never);
  if (expectStats.total === 0 && askStats.total === 0 && neverStats.total === 0) return null;

  return (
    <div className="absolute top-1.5 left-1.5 z-20 flex items-center gap-1.5 px-1 py-0.5">
      <SectionPill icon={Shield} stats={expectStats} />
      <SectionPill icon={MessageCircle} stats={askStats} />
      <SectionPill icon={Ban} stats={neverStats} />
    </div>
  );
}
