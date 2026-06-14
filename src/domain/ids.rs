//! IDs tipados.
//!
//! En vez de pasar `Uuid` desnudos por toda la app (donde es fácil confundir
//! un UserId con un ReportId), envolvemos cada uno en un tipo distinto.
//! El compilador atrapa los cruces.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
        #[sqlx(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self { Self(Uuid::new_v4()) }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

typed_id!(UserId);

impl From<Uuid> for UserId {
    fn from(u: Uuid) -> Self { Self(u) }
}
typed_id!(CompanyId);
typed_id!(ProgramId);
typed_id!(AssetId);
typed_id!(ReportId);
typed_id!(AttachmentId);
typed_id!(PayoutId);
typed_id!(PaymentMethodId);
typed_id!(SessionId);
typed_id!(ReportEventId);
typed_id!(OsintReportId);
typed_id!(CourseRequestId);
