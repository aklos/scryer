import { useEffect, createContext, useContext } from "react";
import { client } from "./index";
import type { ScryerClient } from "./index";

// Create Context
const ScryerContext = createContext<ScryerClient | null>(null);

// Scryer Provider (initializes client)
export function ScryerProvider({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    if (!(window as any).__SCRYER_INITIALIZED__) {
      client.init();
      (window as any).__SCRYER_INITIALIZED__ = true;
    }
  }, []);

  return (
    <ScryerContext.Provider value={client}>{children}</ScryerContext.Provider>
  );
}

// Hook to Access Scryer Client
export function useScryer(): ScryerClient {
  const context = useContext(ScryerContext);
  if (!context) {
    throw new Error("useScryer must be used within a <ScryerProvider>");
  }
  return context;
}
