/** Cumulative authorization roles exposed by the local help desk. */
export type UserRole = "requester" | "technician" | "administrator";

/** Public local account record with no credential material. */
export interface User {
  /** Stable account identifier. */
  id: string;
  /** Normalized local login name. */
  username: string;
  /** Human-facing staff name. */
  display_name: string;
  /** Optional contact metadata that is not used for login. */
  email: string | null;
  /** Current cumulative authorization role. */
  role: UserRole;
  /** Whether the account can sign in. */
  is_active: boolean;
  /** Whether a temporary password must be replaced. */
  must_change_password: boolean;
  /** UTC account creation timestamp. */
  created_at: string;
  /** UTC account update timestamp. */
  updated_at: string;
  /** UTC timestamp of the latest successful login. */
  last_login_at: string | null;
}

/** Approved help-desk ticket priorities. */
export type TicketPriority = "low" | "normal" | "high" | "urgent";

/** Approved help-desk ticket lifecycle states. */
export type TicketStatus = "new" | "open" | "waiting_on_requester" | "resolved" | "closed";

/** Requester-visible or staff-only conversation visibility. */
export type CommentVisibility = "public" | "internal";

/** Public ticket record returned by retained ticket endpoints. */
export interface Ticket {
  /** Stable ticket identifier. */
  id: string;
  /** Sequential human-readable ticket number. */
  number: number;
  /** Concise problem summary. */
  title: string;
  /** Full requester-supplied problem description. */
  description: string;
  /** Account that submitted the ticket. */
  requester_id: string;
  /** Technician currently responsible for the ticket. */
  assignee_id: string | null;
  /** Administrator-configured category identifier. */
  category_id: string | null;
  /** Current lifecycle state. */
  status: TicketStatus;
  /** Current urgency. */
  priority: TicketPriority;
  /** UTC creation timestamp. */
  created_at: string;
  /** UTC last-change timestamp. */
  updated_at: string;
  /** UTC timestamp when the ticket most recently became resolved. */
  resolved_at: string | null;
  /** UTC timestamp when the ticket most recently became closed. */
  closed_at: string | null;
}

/** One public comment or staff-only internal note. */
export interface TicketComment {
  /** Stable comment identifier. */
  id: string;
  /** Parent ticket identifier. */
  ticket_id: string;
  /** Account that authored the entry. */
  author_id: string;
  /** Plain-text entry body. */
  body: string;
  /** Requester or staff visibility boundary. */
  visibility: CommentVisibility;
  /** UTC creation timestamp. */
  created_at: string;
  /** UTC timestamp of the most recent comment mutation. */
  updated_at: string;
}

/** Approved attachment parent types. */
export type AttachmentParentKind = "ticket" | "public_comment" | "internal_note" | "announcement";

/** Public metadata for one safely stored attachment. */
export interface Attachment {
  /** Stable attachment identifier. */
  id: string;
  /** Approved attachment parent kind. */
  parent_kind: AttachmentParentKind;
  /** Approved parent identifier. */
  parent_id: string;
  /** Original human-facing filename. */
  original_name: string;
  /** Detected server-side media type. */
  media_type: string;
  /** Exact file size in bytes. */
  size_bytes: number;
  /** UTC upload timestamp. */
  created_at: string;
}

/** Non-secret runtime configuration available before authentication. */
export interface PublicConfig {
  /** Operator-configured application name. */
  app_name: string;
  /** Optional operator-configured support contact. */
  support_contact: string | null;
  /** Stable public raster logo endpoint when one is configured. */
  logo_url: string | null;
  /** Maximum bytes allowed for one uploaded file. */
  max_upload_bytes: number;
  /** Maximum aggregate attachment bytes allowed on one ticket. */
  max_ticket_upload_bytes: number;
  /** Active requester-selectable ticket categories. */
  categories: PublicCategory[];
  /** Active category preselected for new requests. */
  default_category_id: string | null;
  /** Priority preselected for new requests. */
  default_priority: TicketPriority;
  /** Whether the first administrator must still be created. */
  setup_required: boolean;
  /** Running server package version. */
  version: string;
}

/** One active requester-selectable category from public configuration. */
export interface PublicCategory {
  /** Stable category identifier. */
  id: string;
  /** Human-facing category name. */
  name: string;
  /** Optional explanatory category text. */
  description: string | null;
  /** Administrator-controlled display order. */
  sort_order: number;
}

/** Administrator-controlled staff bulletin lifecycle states. */
export type AnnouncementState = "draft" | "published" | "archived";

/** One local staff announcement returned as unrendered Markdown source. */
export interface Announcement {
  /** Stable announcement identifier. */
  id: string;
  /** Concise staff-facing heading. */
  title: string;
  /** Unrendered Markdown source. */
  body: string;
  /** Administrator who created the record. */
  author_id: string;
  /** Current administrator-controlled lifecycle state. */
  state: AnnouncementState;
  /** Whether the notice sorts before ordinary published items. */
  is_pinned: boolean;
  /** UTC timestamp of first publication. */
  published_at: string | null;
  /** UTC creation timestamp. */
  created_at: string;
  /** UTC latest-mutation timestamp. */
  updated_at: string;
}

/** Input used to create one private announcement draft. */
export interface CreateAnnouncementRequest {
  /** Concise staff-facing heading. */
  title: string;
  /** Unrendered Markdown source. */
  body: string;
  /** Initial pinned state. */
  is_pinned?: boolean;
}

/** Editable fields on a non-archived announcement. */
export interface UpdateAnnouncementRequest {
  /** Optional replacement heading. */
  title?: string;
  /** Optional replacement Markdown source. */
  body?: string;
  /** Optional replacement pinned state. */
  is_pinned?: boolean;
}

/** Private current-account event kinds. */
export type NotificationKind =
  | "ticket_created"
  | "new_ticket"
  | "ticket_comment"
  | "ticket_status_changed"
  | "ticket_resolved"
  | "ticket_reopened"
  | "announcement_published";

/** One bounded in-app notice owned by exactly one account. */
export interface Notification {
  /** Stable notification identifier. */
  id: string;
  /** Account allowed to read this record. */
  user_id: string;
  /** Typed source event. */
  kind: NotificationKind;
  /** Generic event heading. */
  title: string;
  /** Bounded summary without submitted content. */
  body: string;
  /** Optional same-origin application destination. */
  target_path: string | null;
  /** UTC creation timestamp. */
  created_at: string;
  /** UTC first-read timestamp. */
  read_at: string | null;
}

/** Administrator-visible non-secret runtime settings. */
export interface AdminSettings {
  /** Name shown throughout the browser application. */
  app_name: string;
  /** Optional local support contact. */
  support_contact: string | null;
  /** Stable public logo endpoint when configured. */
  logo_url: string | null;
  /** Active category preselected for new tickets. */
  default_category_id: string | null;
  /** Priority preselected for new tickets. */
  default_priority: TicketPriority;
}

/** Partial non-secret settings mutation. */
export interface UpdateAdminSettingsRequest {
  /** Optional replacement application name. */
  app_name?: string;
  /** Optional replacement support contact where blank clears it. */
  support_contact?: string;
  /** Optional replacement default urgency. */
  default_priority?: TicketPriority;
}

/** One administrator-managed ticket category including inactive records. */
export interface Category extends PublicCategory {
  /** Whether requesters may select this category. */
  is_active: boolean;
  /** UTC creation timestamp. */
  created_at: string;
  /** UTC latest-mutation timestamp. */
  updated_at: string;
}

/** Input used to create one active category. */
export interface CreateCategoryRequest {
  /** Unique staff-facing category name. */
  name: string;
  /** Optional explanatory text. */
  description: string | null;
  /** Explicit display order. */
  sort_order?: number;
}

/** Editable category fields. */
export interface UpdateCategoryRequest {
  /** Optional replacement name. */
  name?: string;
  /** Optional replacement description where blank clears it. */
  description?: string;
  /** Optional requester-selectable state. */
  is_active?: boolean;
  /** Optional replacement display order. */
  sort_order?: number;
}

/** Credentials submitted to the built-in local login endpoint. */
export interface LoginRequest {
  /** Normalized local username. */
  username: string;
  /** Plaintext password sent only over the configured origin. */
  password: string;
}

/** First-run administrator details submitted while the database is empty. */
export interface SetupRequest {
  /** Normalized local administrator username. */
  username: string;
  /** Human-facing administrator name. */
  display_name: string;
  /** Initial administrator passphrase. */
  password: string;
}

/** Current-password-confirmed local credential replacement. */
export interface ChangePasswordRequest {
  /** Existing password used to confirm the account holder. */
  current_password: string;
  /** Replacement password subject to local policy. */
  new_password: string;
}

/** Authenticated response whose CSRF secret remains only in process memory. */
export interface AuthSession {
  /** Public account fields for the authenticated staff member. */
  user: User;
  /** Per-session request-integrity secret, never persisted by the browser app. */
  csrf_token: string;
}

/** One bounded server page of public records. */
export interface Page<T> {
  /** Records in the requested page. */
  items: T[];
  /** Effective one-based page number. */
  page: number;
  /** Effective bounded page size. */
  page_size: number;
  /** Total records before pagination. */
  total: number;
}

/** Administrator input for one named local staff account. */
export interface CreateUserRequest {
  /** Normalized local login name. */
  username: string;
  /** Human-facing staff name. */
  display_name: string;
  /** Initial cumulative authorization role. */
  role: UserRole;
  /** Optional contact metadata that is not used for login. */
  email: string | null;
}

/** Administrator-controlled mutable account fields. */
export interface UpdateUserRequest {
  /** Optional replacement human-facing staff name. */
  display_name?: string;
  /** Optional replacement authorization role. */
  role?: UserRole;
  /** Optional account activation state. */
  is_active?: boolean;
  /** Current password required for changes to the signed-in administrator. */
  current_password?: string;
}

/** Account mutation response with optional rotated-session integrity proof. */
export interface UserMutation {
  /** Public account fields after the update. */
  user: User;
  /** Replacement CSRF secret after a signed-in administrator privilege change. */
  csrf_token?: string;
}

/** One-time onboarding material returned only by create, import, and reset. */
export interface OneTimeCredential {
  /** Public account created or reset by the operation. */
  user: User;
  /** Temporary password that must be delivered securely and then discarded. */
  temporary_password: string;
}

/** One validated row from an administrator roster upload. */
export interface RosterRow {
  /** One-based CSV source row. */
  row_number: number;
  /** Normalized local login name. */
  username: string;
  /** Human-facing staff name. */
  display_name: string;
  /** Initial cumulative authorization role. */
  role: UserRole;
  /** Optional contact metadata. */
  email: string | null;
}

/** One safe validation error from a roster preview. */
export interface RosterError {
  /** CSV source row when the error belongs to one row. */
  row_number?: number;
  /** CSV field when the error belongs to one column. */
  field?: string;
  /** Human-facing validation message without credential material. */
  message: string;
}

/** Complete read-only validation result for one roster upload. */
export interface RosterPreview {
  /** Whether the exact CSV can be applied atomically. */
  valid: boolean;
  /** Parsed rows ready for display. */
  rows: RosterRow[];
  /** Validation errors that prevent apply. */
  errors: RosterError[];
}

/** Atomic roster apply result containing credentials shown exactly once. */
export interface RosterApplyResult {
  /** Created accounts in CSV order. */
  created: OneTimeCredential[];
}

/** Data required to submit one help-desk ticket. */
export interface CreateTicketRequest {
  /** Concise problem summary. */
  title: string;
  /** Full problem description. */
  description: string;
  /** Selected administrator-configured category. */
  category_id: string;
  /** Requester-selected urgency. */
  priority: TicketPriority;
}

/** Supported ticket list filters. */
export interface ListTicketsParams {
  /** Optional lifecycle-state filter. */
  status?: TicketStatus;
  /** Optional category filter. */
  category_id?: string;
  /** Optional exact urgency filter. */
  priority?: TicketPriority;
  /** Optional exact technician assignment filter. */
  assignee_id?: string;
  /** Optional case-insensitive title and description search. */
  search?: string;
  /** Opaque continuation cursor returned by the previous page. */
  cursor?: string;
  /** Requested bounded result count. */
  page_size?: number;
}

/** One stable newest-first ticket page. */
export interface TicketPage {
  /** Tickets visible to the signed-in account. */
  items: Ticket[];
  /** Opaque cursor for the next page when more records exist. */
  next_cursor: string | null;
}

/** Partial technician or administrator ticket update. */
export interface UpdateTicketRequest {
  /** Replacement lifecycle state. */
  status?: TicketStatus;
  /** Replacement priority. */
  priority?: TicketPriority;
  /** Replacement technician assignment. */
  assignee_id?: string | null;
  /** Replacement active category. */
  category_id?: string;
  /** Timestamp observed by the client for conflict detection. */
  expected_updated_at?: string;
}

/** Data required to add a ticket conversation entry. */
export interface AddCommentRequest {
  /** Plain-text entry body. */
  body: string;
  /** Public-comment or internal-note visibility. */
  visibility: CommentVisibility;
}

/** Privacy-bounded administrative audit entry. */
export interface AuditEntry {
  /** Stable audit entry identifier. */
  id: string;
  /** Responsible account when it still exists. */
  actor_id: string | null;
  /** Stable machine-readable action name. */
  action: string;
  /** Type of entity affected. */
  target_type: string;
  /** Affected entity identifier when applicable. */
  target_id: string | null;
  /** Short non-sensitive action summary. */
  summary: string;
  /** UTC creation timestamp. */
  created_at: string;
}
