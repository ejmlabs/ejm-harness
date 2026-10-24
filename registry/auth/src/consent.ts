import { APIError, type BetterAuthPlugin, type DBAdapter } from "better-auth";

// Better Auth 1.7.10 consent handlers need a guarded transition under concurrent requests.
export const atomicDeviceConsent: BetterAuthPlugin = {
  id: "ejm-atomic-device-consent",
  init(context) {
    const adapter = context.adapter;
    const update: DBAdapter["update"] = async <T>(
      data: Parameters<DBAdapter["update"]>[0],
    ): Promise<T | null> => {
      if (
        data.model !== "deviceCode" ||
        (data.update.status !== "approved" && data.update.status !== "denied")
      ) {
        return adapter.update<T>(data);
      }
      const userId: unknown = data.update.userId;
      if (typeof userId !== "string") {
        throw new APIError("BAD_REQUEST", { error: "invalid_request" });
      }
      const row = await adapter.incrementOne<T>({
        model: data.model,
        where: [
          ...data.where,
          { field: "status", value: "pending" },
          { field: "userId", value: userId },
        ],
        increment: {},
        set: data.update,
      });
      if (row === null) {
        throw new APIError("BAD_REQUEST", {
          error: "invalid_request",
          error_description: "Device authorization has already been processed.",
        });
      }
      return row;
    };
    return { context: { adapter: { ...adapter, update } } };
  },
};
