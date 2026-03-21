# Agent Image Workflow Design Spec

**Date:** 2026-03-21
**Status:** Draft
**Scope:** ODE 이미지 라이프사이클 감사, 갭 수정, 에이전트용 이미지 워크플로우 가이드 확립

## Context

ODE CLI로 한글 포스터(39노드, 700줄+ JSON)를 생성하는 실험을 수행했다. 텍스트, 프레임, 그라디언트, 드롭 쉐도우 등은 정상 작동했으나 **이미지가 하나도 없는 포스터**가 산출되었다. 로고, 사진 없이는 포스터라고 부르기 어렵다.

이미지 관련 데이터 모델(`ImageSource`, `AssetStore`, `ImageFillMode`)은 이미 구현되어 있으나, 에이전트 관점에서 end-to-end 워크플로우가 검증되지 않았다. 이 스펙은 이미지 라이프사이클 전체를 감사하고, 발견된 갭을 수정하며, 에이전트가 따라할 수 있는 실용 가이드를 확립한다.

## Phase 1: 코드 감사

이미지가 ODE에서 거치는 4단계 라이프사이클을 각각 감사한다.

### 1.1 입력 (Input)

**대상 파일:** `crates/ode-cli/src/mutate.rs`

**확인 사항:**
- `ode add image <file> --src <path>` 실행 시 실제로 어떤 일이 일어나는가
- `--src` 경로가 상대 경로일 때 해석 기준은 무엇인가
- 이미지 파일이 assets/ 디렉토리로 복사되는가, 아니면 경로만 참조하는가
- 지원하는 이미지 포맷 (PNG, JPEG, WebP, SVG 등)

### 1.2 저장 (Storage)

**대상 파일:** `crates/ode-format/src/asset.rs`, `crates/ode-format/src/container.rs`

**확인 사항:**
- `AssetStore`의 `OnDisk` / `Loaded` 모드 전환 로직
- SHA-256 해시 기반 중복 방지가 실제로 동작하는가
- `ode pack` 실행 시 Linked 이미지가 ZIP에 포함되는가
- `ode unpack` 후 이미지가 assets/ 에 정상 추출되는가
- Linked → Embedded 자동 변환이 일어나는가, 아니면 수동으로 해야 하는가

### 1.3 렌더링 (Rendering)

**대상 파일:** `crates/ode-core/src/convert.rs`, `crates/ode-core/src/render.rs`

**확인 사항:**
- Image 노드 → RenderCommand 변환 시 이미지 데이터 로딩 경로
- `ImageFillMode` (Fill, Fit, Crop, Tile) 각각 구현 상태
- 이미지 리사이즈/트랜스폼 적용 로직
- Linked 경로 이미지의 런타임 로딩 시 에러 핸들링

### 1.4 내보내기 (Export)

**대상 파일:** `crates/ode-export/src/png.rs`, `svg.rs`, `pdf.rs`

**확인 사항:**
- PNG 출력에 이미지가 래스터라이즈되는지
- SVG 출력에 이미지가 base64 인라인 또는 외부 참조인지
- PDF 출력에 이미지가 임베딩되는지

## Phase 2: 테스트 및 수정

감사에서 발견된 갭을 실제 이미지로 검증하고 수정한다.

### 테스트 시나리오

| ID | 시나리오 | 입력 | 기대 결과 |
|----|----------|------|-----------|
| T1 | Linked 이미지 삽입 + 렌더링 | 로컬 PNG 파일, document.json에 `ImageSource::Linked` | `ode build` 출력 PNG에 이미지 표시 |
| T2 | 언팩 → 이미지 복사 → 팩 → 렌더링 | assets/ 에 이미지 복사, `ode pack` → `ode build` | packed .ode에서 이미지 렌더링 정상 |
| T3 | 이미지 크기/위치 조정 | width, height, transform 변경 | 이미지가 지정 크기/위치에 렌더링 |

### 수정 원칙

- 기존 코드 구조 존중 — 새 기능 추가보다 **기존 코드가 의도대로 동작하게** 만드는 것 우선
- CLI에 빠진 플래그는 document.json 직접 작성으로 우회 가능한 것은 후순위
- 렌더링 파이프라인 버그 (이미지가 안 보임)는 즉시 수정

### 최종 검증

기존 포스터에 대체 이미지(로고 위치에 테스트 이미지)를 삽입하여 렌더링. PNG 출력에서 이미지가 올바른 위치/크기로 보이면 Phase 2 완료.

## Phase 3: 에이전트 이미지 워크플로우 가이드

감사 + 수정 결과를 에이전트가 바로 따라할 수 있는 가이드로 정리한다.

### 문서 위치

`docs/guides/agent-image-workflow.md` (design-knowledge/ 또는 docs/ 하위)

### 문서 구조

```
1. 이미지 라이프사이클 개요
   - 입력 → 저장 → 렌더링 → 내보내기 흐름
   - Linked vs Embedded: 차이점과 선택 기준

2. 에이전트용 워크플로우 레시피
   - 레시피 A: document.json 직접 작성 (이미지 노드 JSON 스니펫)
   - 레시피 B: CLI로 이미지 추가 (ode add image --src)
   - 레시피 C: 언팩 → 이미지 복사 → 팩 (self-contained .ode 생성)

3. 주의사항 / 알려진 제약
   - 지원 포맷, 크기 제한
   - 감사에서 발견된 제약과 우회 방법

4. 코드 수정 이력
   - 이번 작업에서 수정한 내용과 이유
```

### 작성 원칙

- 에이전트가 이 문서 하나만 읽으면 이미지 작업 가능한 수준
- 코드 내부 설명이 아닌 **"이렇게 하면 된다"** 중심의 실용 가이드
- 복붙 가능한 JSON 스니펫 포함

## Non-Goals

- 이미지 생성 (AI 이미지 생성 등) — ODE는 이미지를 삽입/렌더링하는 도구
- 새로운 이미지 포맷 지원 추가 — 기존 지원 포맷 내에서 작업
- 오토 레이아웃/폰트 개선 — 별도 스펙으로 분리
- MCP 서버 구현 — CLI-Anything 철학 유지
