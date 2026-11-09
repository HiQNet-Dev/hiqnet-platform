import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { AppRouter } from "./app/router";
import { SessionProvider } from "./auth/session";
import "./styles/global.css";

const client = new QueryClient({ defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: false } } });
createRoot(document.getElementById("root")!).render(<StrictMode><QueryClientProvider client={client}><SessionProvider><BrowserRouter><AppRouter /></BrowserRouter></SessionProvider></QueryClientProvider></StrictMode>);
