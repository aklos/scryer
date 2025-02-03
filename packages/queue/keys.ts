import { createEnv } from "@t3-oss/env-nextjs";
import { z } from "zod";

export const keys = () =>
  createEnv({
    server: {
      UPSTASH_REDIS_HOST: z.string().min(1).optional(),
      UPSTASH_REDIS_PASSWORD: z.string().min(1).optional(),
    },
    runtimeEnv: {
      UPSTASH_REDIS_HOST: process.env.UPSTASH_REDIS_HOST,
      UPSTASH_REDIS_PASSWORD: process.env.UPSTASH_REDIS_PASSWORD,
    },
  });
