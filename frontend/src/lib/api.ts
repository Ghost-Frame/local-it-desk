import type {
  AddCommentRequest,
  Attachment,
  AuditEntry,
  CreateTicketRequest,
  ListTicketsParams,
  LoginRequest,
  PublicConfig,
  Ticket,
  TicketComment,
  UpdateTicketRequest,
  User,
  UserRole,
} from "@/types/api";

/** Error thrown when an API request returns a non-success status. */
export class ApiError extends Error {
  /** HTTP response status. */
  readonly status: number;
  /** Parsed response body when one was available. */
  readonly body: unknown;

  /** Creates one bounded API error. */
  constructor(status: number, body: unknown) {
    super(`API error ${status}`);
    this.name = "ApiError";
    this.status = status;
    this.body = body;
  }
}

/** Builds a query string while omitting absent optional values. */
function queryString(params: ListTicketsParams): string {
  const values = new URLSearchParams();
  if (params.status) values.set("status", params.status);
  if (params.category_id) values.set("category_id", params.category_id);
  const encoded = values.toString();
  return encoded ? `?${encoded}` : "";
}

/** Same-origin HTTP client for server-managed cookie sessions. */
class ApiClient {
  /** Sends a JSON request with same-origin cookies. */
  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const response = await fetch(`/api${path}`, {
      method,
      credentials: "same-origin",
      headers: body === undefined ? undefined : { "Content-Type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!response.ok) {
      throw new ApiError(response.status, await response.json().catch(() => null));
    }
    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }

  /** Loads non-secret branding and setup state. */
  async getPublicConfig(): Promise<PublicConfig> {
    const response = await fetch("/api/config", { credentials: "same-origin" });
    if (!response.ok) throw new ApiError(response.status, null);
    return response.json() as Promise<PublicConfig>;
  }

  /** Starts a built-in local account session. */
  async login(credentials: LoginRequest): Promise<User> {
    return this.request("POST", "/auth/login", credentials);
  }

  /** Ends the current local account session. */
  async logout(): Promise<void> {
    return this.request("POST", "/auth/logout");
  }

  /** Loads the current authenticated account. */
  async getCurrentUser(): Promise<User> {
    return this.request("GET", "/users/me");
  }

  /** Lists accounts for administrator-managed assignment controls. */
  async listUsers(): Promise<User[]> {
    return this.request("GET", "/users");
  }

  /** Updates one account role. */
  async updateRole(userId: string, role: UserRole): Promise<void> {
    return this.request("PATCH", `/users/${userId}/role`, { role });
  }

  /** Lists tickets visible to the current account. */
  async listTickets(params: ListTicketsParams = {}): Promise<Ticket[]> {
    return this.request("GET", `/tickets${queryString(params)}`);
  }

  /** Submits a new ticket for the current requester. */
  async createTicket(data: CreateTicketRequest): Promise<Ticket> {
    return this.request("POST", "/tickets", data);
  }

  /** Loads one visible ticket. */
  async getTicket(ticketId: string): Promise<Ticket> {
    return this.request("GET", `/tickets/${ticketId}`);
  }

  /** Applies one technician or administrator ticket update. */
  async updateTicket(ticketId: string, data: UpdateTicketRequest): Promise<Ticket> {
    return this.request("PATCH", `/tickets/${ticketId}`, data);
  }

  /** Lists requester-visible comments and authorized internal notes. */
  async listTicketComments(ticketId: string): Promise<TicketComment[]> {
    return this.request("GET", `/tickets/${ticketId}/comments`);
  }

  /** Adds one public comment or staff-only internal note. */
  async addTicketComment(ticketId: string, data: AddCommentRequest): Promise<TicketComment> {
    return this.request("POST", `/tickets/${ticketId}/comments`, data);
  }

  /** Lists attachments authorized through the parent ticket. */
  async listTicketAttachments(ticketId: string): Promise<Attachment[]> {
    return this.request("GET", `/tickets/${ticketId}/attachments`);
  }

  /** Loads recent privacy-bounded administrative audit entries. */
  async listAuditEntries(): Promise<AuditEntry[]> {
    return this.request("GET", "/admin/audit-log");
  }
}

/** Shared same-origin help-desk API client. */
export const api = new ApiClient();
