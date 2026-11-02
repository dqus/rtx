//! QuakeWorld FTE temp entities have wire layouts independent of NetQuake's type table.
use rtx_proto::protocol::{fte, fte2, ProtoState};
use rtx_proto::sizebuf::Writer;
use rtx_proto::svc::{self, op, SvcEvent, TempEntity, TempEntityKind};

#[test]
fn qw_railtrail_consumes_only_six_coords() {
    let packet = [
        op::TEMP_ENTITY,
        17,
        8,
        0,
        240,
        255,
        24,
        0,
        224,
        255,
        40,
        0,
        208,
        255,
        op::NOP,
    ];
    let events = svc::parse(&mut ProtoState::new(), &packet).unwrap();
    assert_eq!(
        events,
        vec![
            SvcEvent::TempEntity(TempEntity::Trail {
                kind: TempEntityKind::RailTrail,
                start: glam::Vec3::new(1.0, -2.0, 3.0),
                end: glam::Vec3::new(-4.0, 5.0, -6.0),
            }),
            SvcEvent::Nop,
        ]
    );
}

fn proto(float_coords: bool, rd: bool) -> ProtoState {
    let mut proto = ProtoState::new();
    proto.apply(
        if float_coords { fte::FLOATCOORDS } else { 0 },
        if rd { fte2::REPLACEMENTDELTAS } else { 0 },
        0,
    );
    proto
}

fn packet(kind: u8, reference: &[u8], float_coords: bool) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(op::TEMP_ENTITY);
    w.u8(kind);
    w.bytes(reference);
    for coord in [1.25f32, -2.5, 3.75, -4.0, 5.125, -6.25] {
        if float_coords {
            w.bytes(&coord.to_le_bytes());
        } else {
            w.i16((coord * 8.0) as i16);
        }
    }
    w.u8(op::NOP);
    w.into_vec()
}

fn assert_complete_and_truncated(proto: &ProtoState, packet: &[u8], expected: TempEntity) {
    assert_eq!(
        svc::parse(&mut proto.clone(), packet).unwrap(),
        vec![SvcEvent::TempEntity(expected), SvcEvent::Nop],
    );
    // The NOP is outside the TE payload. Every shorter prefix of that payload must underflow.
    for end in 1..packet.len() - 1 {
        assert!(
            matches!(
                svc::parse(&mut proto.clone(), &packet[..end]),
                Err(svc::ParseError::Underflow(_))
            ),
            "truncation {end} of {packet:?}",
        );
    }
}

#[test]
fn railtrail_honors_coord_width_without_entity_or_color_even_with_rd() {
    for float_coords in [false, true] {
        for rd in [false, true] {
            assert_complete_and_truncated(
                &proto(float_coords, rd),
                &packet(17, &[], float_coords),
                TempEntity::Trail {
                    kind: TempEntityKind::RailTrail,
                    start: glam::Vec3::new(1.25, -2.5, 3.75),
                    end: glam::Vec3::new(-4.0, 5.125, -6.25),
                },
            );
        }
    }
}

#[test]
fn qw_beams_use_ordinary_rd_references_not_update_headers() {
    // Ordinary references use 15 bits inline (bit 14 is not an extension flag), or 23 bits
    // with eight low bits in the next byte. Update/removal headers use a different layout.
    let references: &[(u32, &[u8])] = &[
        (0, &[0, 0]),
        (16383, &[255, 63]),
        (16384, &[0, 64]),
        (32767, &[255, 127]),
        (32768, &[128, 128, 0]),
        (65535, &[255, 128, 255]),
        (8388607, &[255, 255, 255]),
    ];
    for float_coords in [false, true] {
        for (raw, kind) in [
            (5, TempEntityKind::Lightning1),
            (6, TempEntityKind::Lightning2),
            (9, TempEntityKind::Lightning3),
            (18, TempEntityKind::GrappleBeam),
        ] {
            for &(entity, reference) in references {
                assert_complete_and_truncated(
                    &proto(float_coords, true),
                    &packet(raw, reference, float_coords),
                    TempEntity::Beam {
                        kind,
                        entity,
                        start: glam::Vec3::new(1.25, -2.5, 3.75),
                        end: glam::Vec3::new(-4.0, 5.125, -6.25),
                    },
                );
            }
        }
    }
}

#[test]
fn legacy_beam_entity_bitpatterns_remain_unsigned_shorts() {
    for float_coords in [false, true] {
        for (raw, kind) in [
            (5, TempEntityKind::Lightning1),
            (6, TempEntityKind::Lightning2),
            (9, TempEntityKind::Lightning3),
            (18, TempEntityKind::GrappleBeam),
        ] {
            // Negative lightning-entity rail aliases (-1, -7) remain the prior raw Beam event;
            // they must not consume an RD extension byte or change the payload interpretation.
            for (reference, entity) in [([0, 128], 32768), ([255, 255], 65535), ([249, 255], 65529)] {
                assert_complete_and_truncated(
                    &proto(float_coords, false),
                    &packet(raw, &reference, float_coords),
                    TempEntity::Beam {
                        kind,
                        entity,
                        start: glam::Vec3::new(1.25, -2.5, 3.75),
                        end: glam::Vec3::new(-4.0, 5.125, -6.25),
                    },
                );
            }
        }
    }
}

#[test]
fn qw_17_and_18_do_not_enable_conflicting_netquake_extensions() {
    for raw in [17, 18] {
        let mut nq = rtx_proto::nq::protocol::NqProtoState::new();
        assert!(matches!(
            rtx_proto::nq::svc::parse(&mut nq, &packet(raw, &[], false)),
            Err(svc::ParseError::UnknownSvc { svc, offset: 1 }) if svc == raw,
        ));
    }
}

#[test]
fn qw_beam_uses_the_existing_grapple_beam_kind() {
    let packet = [
        op::TEMP_ENTITY,
        18,
        7,
        0,
        8,
        0,
        240,
        255,
        24,
        0,
        224,
        255,
        40,
        0,
        208,
        255,
        op::NOP,
    ];
    let events = svc::parse(&mut ProtoState::new(), &packet).unwrap();
    assert_eq!(
        events,
        vec![
            SvcEvent::TempEntity(TempEntity::Beam {
                kind: TempEntityKind::GrappleBeam,
                entity: 7,
                start: glam::Vec3::new(1.0, -2.0, 3.0),
                end: glam::Vec3::new(-4.0, 5.0, -6.0),
            }),
            SvcEvent::Nop,
        ]
    );
}
