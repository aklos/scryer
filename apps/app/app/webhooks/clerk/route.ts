import { env } from "@/env";
import { createHmac } from "crypto";
import { analytics } from "@repo/analytics/posthog/server";
import type {
  DeletedObjectJSON,
  UserJSON,
  WebhookEvent,
} from "@repo/auth/server";
import { log } from "@repo/observability/log";
import { addUser, deleteUser } from "@repo/database";
import { headers } from "next/headers";
import { NextResponse } from "next/server";

const handleUserCreated = async (data: UserJSON) => {
  analytics.identify({
    distinctId: data.id,
    properties: {
      email: data.email_addresses.at(0)?.email_address,
      firstName: data.first_name,
      lastName: data.last_name,
      createdAt: new Date(data.created_at),
      avatar: data.image_url,
      phoneNumber: data.phone_numbers.at(0)?.phone_number,
    },
  });

  analytics.capture({
    event: "User Created",
    distinctId: data.id,
  });

  addUser(data.id);

  return new Response("User created", { status: 201 });
};

const handleUserUpdated = (data: UserJSON) => {
  analytics.identify({
    distinctId: data.id,
    properties: {
      email: data.email_addresses.at(0)?.email_address,
      firstName: data.first_name,
      lastName: data.last_name,
      createdAt: new Date(data.created_at),
      avatar: data.image_url,
      phoneNumber: data.phone_numbers.at(0)?.phone_number,
    },
  });

  analytics.capture({
    event: "User Updated",
    distinctId: data.id,
  });

  return new Response("User updated", { status: 201 });
};

const handleUserDeleted = async (data: DeletedObjectJSON) => {
  if (data.id) {
    analytics.identify({
      distinctId: data.id,
      properties: {
        deleted: new Date(),
      },
    });

    analytics.capture({
      event: "User Deleted",
      distinctId: data.id,
    });

    deleteUser(data.id);
  }

  return new Response("User deleted", { status: 201 });
};

export const POST = async (request: Request): Promise<Response> => {
  if (!env.CLERK_WEBHOOK_SECRET) {
    return NextResponse.json({ message: "Not configured", ok: false });
  }

  // Get the headers
  const headerPayload = await headers();
  const svixId = headerPayload.get("svix-id");
  const svixTimestamp = headerPayload.get("svix-timestamp");
  const svixSignature = headerPayload.get("svix-signature");

  // If there are no headers, error out
  if (!svixId || !svixTimestamp || !svixSignature) {
    return new Response("Error occured -- no svix headers", {
      status: 400,
    });
  }

  // Get the body
  const body = await request.text();

  let event: WebhookEvent | undefined;

  // Verify the payload with the headers
  try {
    const signatures = svixSignature.split(" ").map((sig) => {
      const [version, signature] = sig.split(",");
      if (!version || !signature) {
        throw new Error(`Invalid signature format: ${sig}`);
      }
      return { version, signature };
    });
    const signedContent = `${svixId}.${svixTimestamp}.${body}`;
    const secretBytes = Buffer.from(
      env.CLERK_WEBHOOK_SECRET.split("_")[1],
      "base64"
    );

    let isValid = false;

    for (const { version, signature } of signatures) {
      let expectedSignature: string;

      switch (version) {
        case "v1":
          expectedSignature = createHmac("sha256", secretBytes)
            .update(signedContent)
            .digest("base64");
          break;
        case "v2":
          // If v2 uses a different algorithm or encoding, implement it here.
          expectedSignature = createHmac("sha256", secretBytes)
            .update(signedContent)
            .digest("base64");
          break;
        default:
          log.warn(`Unsupported signature version: ${version}`);
          continue; // Skip unsupported versions
      }

      if (expectedSignature === signature) {
        isValid = true;
        break;
      }
    }

    if (!isValid) {
      throw new Error("Signature verification failed");
    }

    event = JSON.parse(body) as WebhookEvent;
  } catch (error) {
    log.error("Error verifying webhook:", { error });
    return new Response("Error occured", {
      status: 400,
    });
  }

  // Get the ID and type
  const { id } = event.data;
  const eventType = event.type;

  log.info("Webhook", { id, eventType, body });

  let response: Response = new Response("", { status: 201 });

  switch (eventType) {
    case "user.created": {
      response = await handleUserCreated(event.data);
      break;
    }
    case "user.updated": {
      response = handleUserUpdated(event.data);
      break;
    }
    case "user.deleted": {
      response = await handleUserDeleted(event.data);
      break;
    }
    default: {
      break;
    }
  }

  await analytics.shutdown();

  return response;
};
