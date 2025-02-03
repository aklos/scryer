import { NextRequest } from "next/server";
import { currentUser } from "@repo/auth/server";
import { getReminders, getTasks } from "@repo/database";

export async function GET(request: NextRequest) {
  const user = await currentUser();

  if (!user) {
    return Response.json({ ok: false }, { status: 403 });
  }

  const tasks = await getTasks(user.id);
  const reminders = await getReminders(user.id);

  return Response.json({ ok: true, tasks, reminders }, { status: 200 });
}
