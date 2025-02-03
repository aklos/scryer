import { auth } from "@repo/auth/server";
import type { Metadata } from "next";
import dynamic from "next/dynamic";
import { notFound } from "next/navigation";
import Chat from "./chat";

const title = "Touchbase";
const description = "Personal AI assistant.";

export const metadata: Metadata = {
  title,
  description,
};

const App = async () => {
  // const pages = await database.page.findMany();
  const { userId } = await auth();

  if (!userId) {
    notFound();
  }

  return (
    <>
      <Chat />
      {/* <div className="flex flex-1 flex-col gap-4 p-4 pt-0">
        <div className="grid auto-rows-min gap-4 md:grid-cols-3">
          {pages.map((page) => (
            <div key={page.id} className="aspect-video rounded-xl bg-muted/50">
              {page.name}
            </div>
          ))}
        </div>
        <div className="min-h-[100vh] flex-1 rounded-xl bg-muted/50 md:min-h-min" />
      </div> */}
    </>
  );
};

export default App;
