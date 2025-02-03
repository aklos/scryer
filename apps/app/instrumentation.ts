import { initializeSentry } from "@repo/observability/instrumentation";

export async function register() {
  initializeSentry();
  if (
    process.env.NODE_ENV === "production" &&
    process.env.NEXT_RUNTIME === "nodejs"
  ) {
    // await import("@repo/messaging");
    const { getTelegramBot } = require("@repo/messaging");
    getTelegramBot();
    await import("./messagingHandlers");
  }
}
