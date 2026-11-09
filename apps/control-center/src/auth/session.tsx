import { createContext, useContext, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { controlClient, controlSessionEvents, type Capability, type StaffMe } from "../api/control-client";

export type SessionState =
  | { kind: "loading" }
  | { kind: "anonymous" }
  | { kind: "network-error"; retry: () => void }
  | { kind: "authenticated"; staff: StaffMe };

const SessionContext = createContext<SessionState>({ kind: "loading" });

export function SessionProvider({ children }: { children: React.ReactNode }) {
  const client = useQueryClient();
  const [expired, setExpired] = useState(false);
  const query = useQuery({ queryKey: ["control", "session"], queryFn: controlClient.me, retry: false });
  useEffect(() => {
    const expire = () => { setExpired(true); client.removeQueries({ queryKey: ["control"] }); };
    const restore = () => { setExpired(false); void client.invalidateQueries({ queryKey: ["control", "session"] }); };
    window.addEventListener(controlSessionEvents.expired, expire);
    window.addEventListener(controlSessionEvents.restored, restore);
    return () => { window.removeEventListener(controlSessionEvents.expired, expire); window.removeEventListener(controlSessionEvents.restored, restore); };
  }, [client]);
  const state: SessionState = expired
    ? { kind: "anonymous" }
    : query.isPending
    ? { kind: "loading" }
    : query.isError
      ? (query.error as { status?: number }).status === 401
        ? { kind: "anonymous" }
        : { kind: "network-error", retry: () => void query.refetch() }
      : { kind: "authenticated", staff: query.data };
  return <SessionContext.Provider value={state}>{children}</SessionContext.Provider>;
}

export function useSession() {
  return useContext(SessionContext);
}

export function useCapability(capability: Capability) {
  const session = useSession();
  return session.kind === "authenticated" && session.staff.capabilities.includes(capability);
}

export function useRefreshSession() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["control", "session"] });
}
