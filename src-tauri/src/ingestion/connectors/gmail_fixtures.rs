//! Local dev-only fake Gmail data.
//!
//! Compiled only in debug builds (`#[cfg(debug_assertions)]`) so it ships with
//! every local `tauri dev` / `cargo build` but never a release artifact. The
//! `GmailConnector::sync` skeleton returns these instead of an empty vec while
//! the real Gmail API integration (INTEGRATION-TODO US-1.4) is pending.
//!
//! Each item uses a stable `external_id` and a fixed `collected_at`, so repeated
//! collects are idempotent: the ingestion dedup gate (`ingestion.seen`) skips
//! items it has already processed instead of creating duplicate Facts.

use chrono::{TimeZone, Utc};

use crate::core::types::{RawItem, SourceKind};

/// A fixed base timestamp (2026-01-01T00:00:00Z) so fixtures are deterministic.
const BASE_TS: i64 = 1_767_225_600;

fn fixture(external_id: &str, offset_days: i64, text: &str) -> RawItem {
    RawItem {
        source: SourceKind::Gmail,
        external_id: external_id.to_string(),
        collected_at: Utc
            .timestamp_opt(BASE_TS + offset_days * 86_400, 0)
            .unwrap(),
        text: Some(text.to_string()),
        image_png: None,
    }
}

/// Fake Gmail messages for local development.
pub fn fake_gmail_items() -> Vec<RawItem> {
    vec![
        // 1. AWS x DDThon 2026 행사 방문 안내 메일
        fixture(
            "dev-gmail-ddthon-2026",
            0,
            "\
From: AWS x DDThon 운영팀 <events@ddthon.dev>
To: 나 <me@example.com>
Subject: [AWS x DDThon 2026] 오프라인 행사 방문 안내 및 일정

안녕하세요, DDThon 참가자님.

AWS와 함께하는 DDThon 2026 오프라인 행사에 신청해 주셔서 감사합니다.
아래 일정으로 방문해 주시기 바랍니다.

- 행사명: AWS x DDThon 2026 (Data-Driven Hackathon)
- 일시: 2026년 3월 14일(토) 오전 10시 ~ 오후 6시
- 장소: AWS Korea 세미나실, 서울 강남구 테헤란로
- 준비물: 노트북, 참가 확인 QR 코드
- 트랙: Bedrock 기반 생성형 AI 애플리케이션, 서버리스 데이터 파이프라인

당일 오전 9시 30분부터 등록 데스크에서 체크인이 가능합니다.
점심과 저녁 식사가 제공되며, 우수 팀에게는 AWS 크레딧이 지급됩니다.

행사에서 뵙겠습니다.
AWS x DDThon 운영팀 드림",
        ),
        // 2. LinkedIn — IT 기업 관련 소식
        fixture(
            "dev-gmail-linkedin-it-news",
            2,
            "\
From: LinkedIn <news@linkedin.com>
To: 나 <me@example.com>
Subject: 회원님이 팔로우하는 IT 기업들의 이번 주 소식

이번 주 회원님의 관심 분야인 IT 업계 주요 소식입니다.

- 네이버클라우드, 자체 개발 LLM 'HyperCLOVA X' 신규 버전 공개
- 카카오, 사내 개발 생산성 향상을 위한 AI 코딩 어시스턴트 전사 도입
- 토스, 백엔드 엔지니어 및 ML 엔지니어 대규모 채용 시작
- 당근마켓, 실시간 추천 시스템을 Kafka 기반으로 전면 재구축
- 쿠팡, 물류 자동화를 위한 컴퓨터 비전 팀 신설

회원님과 연결된 12명이 이 소식에 반응했습니다.
더 많은 소식은 LinkedIn 피드에서 확인하세요.",
        ),
        // 3. SK하이닉스 AI/DT 지원 불합격 안내
        fixture(
            "dev-gmail-hynix-reject",
            5,
            "\
From: SK하이닉스 채용팀 <recruit@skhynix.com>
To: 나 <me@example.com>
Subject: [SK하이닉스] 2026 상반기 AI/DT 직무 지원 결과 안내

안녕하세요, 지원자님.

먼저 SK하이닉스 2026년 상반기 AI/DT(Data Transformation) 직무에
지원해 주셔서 진심으로 감사드립니다.

지원해 주신 서류를 신중히 검토하였으나, 아쉽게도 이번 채용에서는
함께하지 못하게 되었음을 안내드립니다.

지원자님의 역량과 경험은 충분히 인상적이었으나, 제한된 채용 인원과
직무 적합성을 종합적으로 고려한 결과임을 양해 부탁드립니다.

앞으로 더 좋은 기회로 다시 뵐 수 있기를 기대하며,
지원자님의 앞날에 좋은 결과가 함께하기를 응원합니다.

SK하이닉스 채용팀 드림",
        ),
        // 4. 구독 결제 내역 (YouTube Premium / Netflix / Watcha)
        fixture(
            "dev-gmail-subscription-youtube",
            7,
            "\
From: YouTube Premium <no-reply@youtube.com>
To: 나 <me@example.com>
Subject: YouTube Premium 결제 영수증

YouTube Premium 멤버십 결제가 완료되었습니다.

- 상품: YouTube Premium (개인)
- 결제 금액: 14,900원
- 결제일: 2026년 1월 8일
- 결제 수단: 신용카드 (****-1234)
- 다음 결제 예정일: 2026년 2월 8일

이용해 주셔서 감사합니다.",
        ),
        fixture(
            "dev-gmail-subscription-netflix",
            7,
            "\
From: Netflix <info@netflix.com>
To: 나 <me@example.com>
Subject: Netflix 멤버십 결제 안내

Netflix 멤버십 요금이 청구되었습니다.

- 멤버십: 프리미엄 (4K UHD, 4인 동시 시청)
- 결제 금액: 17,000원
- 결제일: 2026년 1월 8일
- 결제 수단: 신용카드 (****-1234)
- 다음 결제 예정일: 2026년 2월 8일

Netflix를 즐겨주셔서 감사합니다.",
        ),
        fixture(
            "dev-gmail-subscription-watcha",
            7,
            "\
From: 왓챠 <help@watcha.com>
To: 나 <me@example.com>
Subject: [왓챠] 구독 결제가 완료되었습니다

왓챠 구독 결제 내역을 안내드립니다.

- 이용권: 왓챠 베이직 (1인)
- 결제 금액: 7,900원
- 결제일: 2026년 1월 8일
- 결제 수단: 신용카드 (****-1234)
- 다음 결제 예정일: 2026년 2월 8일

왓챠와 함께해 주셔서 감사합니다.",
        ),
        // 5. 삿포로 여행 — 항공권 결제 + 호텔 결제 내역
        fixture(
            "dev-gmail-sapporo-flight",
            10,
            "\
From: 대한항공 <ticket@koreanair.com>
To: 나 <me@example.com>
Subject: [대한항공] 삿포로(CTS) 항공권 예약 및 결제 완료

항공권 예약이 확정되었습니다.

- 예약번호: KE-8XQ2LM
- 여정: 인천(ICN) → 삿포로 신치토세(CTS)
- 출발: 2026년 2월 20일(금) 09:20
- 귀국: 2026년 2월 24일(화) 14:05, 삿포로(CTS) → 인천(ICN)
- 좌석: 이코노미, 2인
- 결제 금액: 872,400원
- 결제 수단: 신용카드 (****-1234)

즐거운 여행 되세요. 대한항공 드림",
        ),
        fixture(
            "dev-gmail-sapporo-hotel",
            10,
            "\
From: 아고다 <booking@agoda.com>
To: 나 <me@example.com>
Subject: [Agoda] 삿포로 호텔 예약 확정 및 결제 영수증

호텔 예약이 확정되었습니다.

- 호텔: 삿포로 스스키노 그랜드 호텔
- 주소: 일본 홋카이도 삿포로시 주오구 스스키노
- 체크인: 2026년 2월 20일(금) 15:00
- 체크아웃: 2026년 2월 24일(화) 11:00
- 객실: 트윈룸 (금연), 4박
- 결제 금액: 512,800원
- 결제 수단: 신용카드 (****-1234)

즐거운 숙박 되세요. 아고다 드림",
        ),
    ]
}
