import type {
  AddCommentRequest,
  AdminSettings,
  Announcement,
  Attachment,
  AuthSession,
  AuditEntry,
  ChangePasswordRequest,
  Category,
  CreateAnnouncementRequest,
  CreateCategoryRequest,
  CreateTicketRequest,
  CreateUserRequest,
  ListTicketsParams,
  LoginRequest,
  Notification,
  OneTimeCredential,
  Page,
  PublicConfig,
  RosterApplyResult,
  RosterPreview,
  SetupRequest,
  Ticket,
  TicketComment,
  TicketPage,
  UpdateTicketRequest,
  UpdateAdminSettingsRequest,
  UpdateAnnouncementRequest,
  UpdateCategoryRequest,
  UpdateUserRequest,
  User,
  UserMutation,
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
  if (params.priority) values.set("priority", params.priority);
  if (params.category_id) values.set("category_id", params.category_id);
  if (params.assignee_id) values.set("assignee_id", params.assignee_id);
  if (params.search) values.set("search", params.search);
  if (params.cursor) values.set("cursor", params.cursor);
  if (params.page_size) values.set("page_size", String(params.page_size));
  const encoded = values.toString();
  return encoded ? `?${encoded}` : "";
}

/** Same-origin HTTP client for server-managed cookie sessions. */
export class ApiClient {
  /** Per-session CSRF secret retained only by this in-memory client instance. */
  private csrfToken: string | null = null;

  /** Sends a same-origin request with transient request-integrity proof. */
  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    contentType = "application/json",
  ): Promise<T> {
    const headers = new Headers();
    if (body !== undefined) headers.set("Content-Type", contentType);
    if (this.csrfToken && !["GET", "HEAD", "OPTIONS"].includes(method)) {
      headers.set("X-CSRF-Token", this.csrfToken);
    }
    const response = await fetch(`/api${path}`, {
      method,
      credentials: "same-origin",
      headers,
      body:
        body === undefined
          ? undefined
          : contentType === "application/json"
            ? JSON.stringify(body)
            : String(body),
    });
    if (!response.ok) {
      throw new ApiError(response.status, await response.json().catch(() => null));
    }
    if (response.status === 204) return undefined as T;
    return response.json() as Promise<T>;
  }

  /** Accepts fresh public identity and replaces the transient CSRF secret. */
  private acceptSession(session: AuthSession): User {
    this.csrfToken = session.csrf_token;
    return session.user;
  }

  /** Accepts an optional CSRF rotation from a signed-in account mutation. */
  private acceptMutation(mutation: UserMutation): User {
    if (mutation.csrf_token) this.csrfToken = mutation.csrf_token;
    return mutation.user;
  }

  /** Clears all browser-process authentication material held by the client. */
  clearAuthentication(): void {
    this.csrfToken = null;
  }

  /** Loads non-secret branding and setup state. */
  async getPublicConfig(): Promise<PublicConfig> {
    const response = await fetch("/api/config", { credentials: "same-origin" });
    if (!response.ok) throw new ApiError(response.status, null);
    return response.json() as Promise<PublicConfig>;
  }

  /** Creates the first administrator and accepts the issued local session. */
  async setup(details: SetupRequest): Promise<User> {
    const session = await this.request<AuthSession>("POST", "/setup", details);
    return this.acceptSession(session);
  }

  /** Starts a built-in local account session. */
  async login(credentials: LoginRequest): Promise<User> {
    const session = await this.request<AuthSession>("POST", "/auth/login", credentials);
    return this.acceptSession(session);
  }

  /** Ends the current local account session. */
  async logout(): Promise<void> {
    try {
      await this.request<void>("POST", "/auth/logout");
    } finally {
      this.clearAuthentication();
    }
  }

  /** Resolves the current cookie session and restores transient CSRF state. */
  async getCurrentSession(): Promise<User> {
    const session = await this.request<AuthSession>("GET", "/auth/session");
    return this.acceptSession(session);
  }

  /** Replaces the current password and accepts the rotated local session. */
  async changePassword(details: ChangePasswordRequest): Promise<User> {
    const session = await this.request<AuthSession>("POST", "/auth/password", details);
    return this.acceptSession(session);
  }

  /** Lists a bounded page of administrator-visible local accounts. */
  async listUsers(page = 1, pageSize = 100): Promise<Page<User>> {
    return this.request("GET", `/admin/users?page=${page}&page_size=${pageSize}`);
  }

  /** Creates one forced-change local account and returns its password once. */
  async createUser(details: CreateUserRequest): Promise<OneTimeCredential> {
    return this.request("POST", "/admin/users", details);
  }

  /** Updates one local account while accepting any self-session rotation. */
  async updateUser(userId: string, details: UpdateUserRequest): Promise<User> {
    const mutation = await this.request<UserMutation>("PATCH", `/admin/users/${userId}`, details);
    return this.acceptMutation(mutation);
  }

  /** Resets one account password and returns its replacement exactly once. */
  async resetUserPassword(userId: string): Promise<OneTimeCredential> {
    return this.request("POST", `/admin/users/${userId}/reset-password`);
  }

  /** Revokes every active session for one local account. */
  async revokeUserSessions(userId: string): Promise<void> {
    return this.request("DELETE", `/admin/users/${userId}/sessions`);
  }

  /** Validates one CSV roster without changing persisted accounts. */
  async previewRoster(csv: string): Promise<RosterPreview> {
    return this.request("POST", "/admin/users/import/preview", csv, "text/csv");
  }

  /** Applies one previously previewed roster as a single transaction. */
  async applyRoster(csv: string): Promise<RosterApplyResult> {
    return this.request("POST", "/admin/users/import/apply", csv, "text/csv");
  }

  /** Lists tickets visible to the current account. */
  async listTickets(params: ListTicketsParams = {}): Promise<TicketPage> {
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

  /** Streams one bounded file to an authorized ticket or conversation parent. */
  async uploadAttachment(
    parentKind: Attachment["parent_kind"],
    parentId: string,
    file: File,
  ): Promise<Attachment> {
    const form = new FormData();
    form.set("parent_kind", parentKind);
    form.set("parent_id", parentId);
    form.set("file", file, file.name);
    const headers = new Headers();
    if (this.csrfToken) headers.set("X-CSRF-Token", this.csrfToken);
    const response = await fetch("/api/attachments", {
      method: "POST",
      credentials: "same-origin",
      headers,
      body: form,
    });
    if (!response.ok) {
      throw new ApiError(response.status, await response.json().catch(() => null));
    }
    return response.json() as Promise<Attachment>;
  }

  /** Lists published staff announcements in server-defined display order. */
  async listAnnouncements(): Promise<Announcement[]> {
    return this.request("GET", "/announcements");
  }

  /** Lists every announcement lifecycle state for an administrator. */
  async listAdminAnnouncements(): Promise<Announcement[]> {
    return this.request("GET", "/admin/announcements");
  }

  /** Creates one administrator-only announcement draft. */
  async createAnnouncement(data: CreateAnnouncementRequest): Promise<Announcement> {
    return this.request("POST", "/admin/announcements", data);
  }

  /** Updates editable content or pin state on one announcement. */
  async updateAnnouncement(
    announcementId: string,
    data: UpdateAnnouncementRequest,
  ): Promise<Announcement> {
    return this.request(
      "PATCH",
      "/admin/announcements/" + encodeURIComponent(announcementId),
      data,
    );
  }

  /** Publishes one draft announcement. */
  async publishAnnouncement(announcementId: string): Promise<Announcement> {
    return this.request(
      "POST",
      "/admin/announcements/" + encodeURIComponent(announcementId) + "/publish",
      {},
    );
  }

  /** Archives one draft or published announcement. */
  async archiveAnnouncement(announcementId: string): Promise<Announcement> {
    return this.request(
      "POST",
      "/admin/announcements/" + encodeURIComponent(announcementId) + "/archive",
      {},
    );
  }

  /** Lists the current account's newest private notifications. */
  async listNotifications(): Promise<Notification[]> {
    return this.request("GET", "/notifications");
  }

  /** Returns the current account's unread notification count. */
  async getUnreadNotificationCount(): Promise<number> {
    const response = await this.request<{ count: number }>("GET", "/notifications/unread-count");
    return response.count;
  }

  /** Idempotently marks one owned notification read. */
  async markNotificationRead(notificationId: string): Promise<void> {
    return this.request(
      "POST",
      "/notifications/" + encodeURIComponent(notificationId) + "/read",
      {},
    );
  }

  /** Marks all current-account notifications read. */
  async markAllNotificationsRead(): Promise<void> {
    return this.request("POST", "/notifications/read-all", {});
  }

  /** Loads administrator-visible non-secret settings. */
  async getAdminSettings(): Promise<AdminSettings> {
    return this.request("GET", "/admin/settings");
  }

  /** Applies one partial non-secret settings update. */
  async updateAdminSettings(data: UpdateAdminSettingsRequest): Promise<AdminSettings> {
    return this.request("PATCH", "/admin/settings", data);
  }

  /** Lists active and inactive administrator-managed categories. */
  async listCategories(): Promise<Category[]> {
    return this.request("GET", "/admin/categories");
  }

  /** Creates one active ticket category. */
  async createCategory(data: CreateCategoryRequest): Promise<Category> {
    return this.request("POST", "/admin/categories", data);
  }

  /** Updates one administrator-managed category. */
  async updateCategory(categoryId: string, data: UpdateCategoryRequest): Promise<Category> {
    return this.request("PATCH", "/admin/categories/" + encodeURIComponent(categoryId), data);
  }

  /** Selects one active category as the requester default. */
  async selectDefaultCategory(categoryId: string): Promise<AdminSettings> {
    return this.request(
      "POST",
      "/admin/categories/" + encodeURIComponent(categoryId) + "/default",
      {},
    );
  }

  /** Uploads and activates one server-validated raster application logo. */
  async uploadLogo(file: File): Promise<AdminSettings> {
    const form = new FormData();
    form.set("file", file, file.name);
    const headers = new Headers();
    if (this.csrfToken) headers.set("X-CSRF-Token", this.csrfToken);
    const response = await fetch("/api/admin/settings/logo", {
      method: "POST",
      credentials: "same-origin",
      headers,
      body: form,
    });
    if (!response.ok) {
      throw new ApiError(response.status, await response.json().catch(() => null));
    }
    return response.json() as Promise<AdminSettings>;
  }

  /** Returns the same-origin download path for one authorized attachment. */
  attachmentUrl(attachmentId: string): string {
    return `/api/attachments/${encodeURIComponent(attachmentId)}`;
  }

  /** Loads recent privacy-bounded administrative audit entries. */
  async listAuditEntries(page = 1, pageSize = 100): Promise<Page<AuditEntry>> {
    return this.request("GET", `/admin/audit-log?page=${page}&page_size=${pageSize}`);
  }
}

/** Shared same-origin help-desk API client. */
export const api = new ApiClient();
