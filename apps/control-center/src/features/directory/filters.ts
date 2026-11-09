export type DirectoryFilterChange = {
  status?: string;
  search?: string;
  identityStatus?: string;
  organizationId?: string;
  productId?: string;
};

const filterKeys = {
  status: "status",
  search: "search",
  identityStatus: "identity_status",
  organizationId: "organization_id",
  productId: "product_id",
} as const;

export function applyDirectoryFilter(
  params: URLSearchParams,
  change: DirectoryFilterChange,
): URLSearchParams {
  const next = new URLSearchParams(params);
  for (const key of Object.keys(filterKeys) as (keyof DirectoryFilterChange)[]) {
    if (!(key in change)) continue;
    const raw = change[key];
    const value = key === "search" ? raw?.trim() : raw;
    if (value) next.set(filterKeys[key], value);
    else next.delete(filterKeys[key]);
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
