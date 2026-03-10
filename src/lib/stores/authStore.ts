export type AuthStatus = "unauthenticated" | "authenticating" | "authenticated" | "error";

export interface Credentials {
  authType: "oauth" | "pat";
  token: string;
  enterpriseUrl?: string;
}

function createAuthStore() {
  let status = $state<AuthStatus>("unauthenticated");
  let credentials = $state<Credentials | null>(null);
  let error = $state<string | null>(null);

  return {
    get status() {
      return status;
    },
    get credentials() {
      return credentials;
    },
    get error() {
      return error;
    },
    get isAuthenticated() {
      return status === "authenticated";
    },

    setStatus(value: AuthStatus) {
      status = value;
    },

    setCredentials(value: Credentials | null) {
      credentials = value;
      if (value) {
        status = "authenticated";
        error = null;
      }
    },

    setError(message: string) {
      status = "error";
      error = message;
    },

    reset() {
      status = "unauthenticated";
      credentials = null;
      error = null;
    },
  };
}

export const authStore = createAuthStore();
