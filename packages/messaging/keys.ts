import { createEnv } from "@t3-oss/env-nextjs";
import { z } from "zod";

export const keys = () =>
  createEnv({
    server: {
      TELEGRAM_BOT_TOKEN: z.string().min(1),
    },
    runtimeEnv: {
      TELEGRAM_BOT_TOKEN: process.env.TELEGRAM_BOT_TOKEN,
    },
  });
