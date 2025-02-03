import { env } from "@/env";
import { config, withAnalyzer } from "@repo/next-config";
import { withLogtail, withSentry } from "@repo/observability/next-config";
import type { NextConfig } from "next";

let nextConfig: NextConfig = {
  ...config,
  output: "standalone",
};

// if (env.VERCEL) {
//   nextConfig = withSentry(nextConfig);
// }

if (env.ANALYZE === "true") {
  nextConfig = withAnalyzer(nextConfig);
}

export default nextConfig;
