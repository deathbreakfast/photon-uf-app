use valence::prelude::*;
use valence::privacy_policies::common::BLOCK_ALL;
use crate::server::{PHOTON_REVEALED_BY_VIEWER, PHOTON_TOPIC_OWNER_READ};

valence_schema! {
    PhotonEventReveal {
        repository: "https://github.com/unified-field-dev/photon-uf-app",
        table: "photon_event_reveal",
        version: "0.1.0",
        description: "Reveal log: who was shown an event's personal fields, when and why",

        policies: {
            read: { allow: [PHOTON_REVEALED_BY_VIEWER, PHOTON_TOPIC_OWNER_READ] },
            create: { allow: [PHOTON_REVEALED_BY_VIEWER] },
            update: { always_block: [BLOCK_ALL] },
            delete: { always_block: [BLOCK_ALL] },
        },

        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            topic_name: { r#type: FieldType::String, required: true },
            event_id: { r#type: FieldType::String, required: true },
            revealed_by: { r#type: FieldType::Record("user"), required: false },
            revealed_at: { r#type: FieldType::DateTime, required: true },
            reason: {
                r#type: FieldType::Enum(&["support", "debugging", "data_subject_request"]),
                required: true,
            },
            details: { r#type: FieldType::String, required: true },
        ],

        connections: [
            revealed_by: {
                table: "user",
                cardinality: HasOne,
                required: false,
                on_delete: SetNull,
                model: "lepton::generated::User",
            },
        ],
    }
}
