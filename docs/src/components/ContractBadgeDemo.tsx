/**
 * Wraps the actual ContractBadge from the app for docs display.
 */
import { ContractBadge } from "@app/nodes/ContractBadge";
import type { Contract } from "@app/types";

export function ContractBadgeDemo({ contract }: { contract: Contract }) {
  return (
    <div className="flex items-center justify-center py-4">
      <ContractBadge contract={contract} inline />
    </div>
  );
}
