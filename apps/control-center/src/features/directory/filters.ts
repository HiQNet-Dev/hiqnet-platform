export type DirectoryFilterChange = { status?: string; search?: string };

export function applyDirectoryFilter(
  params: URLSearchParams,
  change: DirectoryFilterChange,
): URLSearchParams {
  const next = new URLSearchParams(params);
  if (change.status !== undefined) {
    if (change.status) next.set("status", change.status);
    else next.delete("status");
  }
  if (change.search !== undefined) {
    const search = change.search.trim();
    if (search) next.set("search", search);
    else next.delete("search");
  }
  next.delete("cursor");
  return next;
}

export function withDirectoryCursor(
  params: URLSearchParams,
  cursor: string | null | undefined,
): URLSearchParams {
  const next = new URLSearchParams(params);
  if (cursor) next.set("cursor", cursor);
  else next.delete("cursor");
  return next;
}
