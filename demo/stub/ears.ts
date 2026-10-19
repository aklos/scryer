/** Whether a claim names a trigger, state or failure a test exercises —
 *  the same leading-keyword read as `scryer_core::ears::classify`. */
export function classifyTestable(statement: string): boolean {
  return /^(when|while|if)\s/i.test(statement.trim());
}
