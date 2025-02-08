import crypto from "crypto";
import { Queue, Worker } from "bullmq";
import { handleEvent, EventData } from "@repo/events";
import { keys } from "./keys";
// import * as Sentry from "@sentry/nextjs";

const redisConnection = {
  host: keys().UPSTASH_REDIS_HOST,
  port: 16379,
  password: keys().UPSTASH_REDIS_PASSWORD,
  family: 6,
};

const queues = {};
const workers: Worker[] = [];

type JobConfig = {
  removeOnComplete: boolean;
  removeOnFail: boolean;
  attempts?: number;
  backoff?: {
    type: string;
    delay: number;
  };
};

export async function enqueueEvent(accountId: string, data: EventData) {
  const key = accountId;

  if (!queues[key]) {
    createQueue(key, handleEvent);
  }

  await addJob(
    {
      accountId,
      data: Object.assign({}, data),
    },
    key
  );
}

export function createQueue(key: string, handler: Function) {
  const queue = new Queue(key, {
    connection: redisConnection,
  });

  queues[key] = queue;

  createWorker(key, handler);
}

export async function addJob(
  payload: { accountId: string; data: EventData },
  key: string,
  retry: boolean = false
) {
  const queue: Queue = queues[key];

  if (!queue) {
    return;
  }

  const config: JobConfig = {
    removeOnComplete: true,
    removeOnFail: true,
  };

  if (retry) {
    config.attempts = 3;
    config.backoff = {
      type: "exponential",
      delay: 1000,
    };
  }

  const hash = crypto
    .createHash("md5")
    .update(JSON.stringify(payload))
    .digest("hex");

  const existingJob = await queue.getJob(hash);

  if (!existingJob) {
    await queue.add(hash, payload, config);
  }
}

function createWorker(key: string, handler: Function) {
  const worker = new Worker(
    key,
    async (job) => {
      await handler(job.data);
    },
    { connection: redisConnection }
  );

  worker.on("completed", async (job) => {
    await deleteQueueIfEmpty(worker, key);
  });

  worker.on("failed", async (job, err: any) => {
    // Sentry.captureException(err);
    await deleteQueueIfEmpty(worker, key);
  });

  workers.push(worker);
}

async function deleteQueueIfEmpty(worker: Worker, key: string) {
  const queue: Queue = queues[key];
  const waitingJobs = await queue.getWaitingCount();
  const activeJobs = await queue.getActiveCount();

  if (waitingJobs + activeJobs === 0) {
    await queue.pause();
    await queue.obliterate({ force: true });
    await queue.close();
    delete queues[key];

    await worker.close(true);
    const workerIndex = workers.findIndex((w) => w.id === worker.id);
    workers.splice(workerIndex, 1);
  }
}
