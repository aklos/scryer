import { createEnv } from "@t3-oss/env-nextjs";
import { z } from "zod";

export const keys = () =>
  createEnv({
    server: {
      GEOIP_ACCOUNT_ID: z.string().min(1).optional(),
      GEOIP_LICENSE_KEY: z.string().min(1).optional(),
    },
    runtimeEnv: {
      GEOIP_ACCOUNT_ID: process.env.GEOIP_ACCOUNT_ID,
      GEOIP_LICENSE_KEY: process.env.GEOIP_LICENSE_KEY,
    },
  });
