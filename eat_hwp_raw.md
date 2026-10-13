# Hwp Ai Agent
LLM이 현재 HWP Document의 양식과 내용, 기초자료를 바탕으로 문서를 작성하거나
수정하는 HWP AI AGENT.

## Features

1. 현재 XHwpDocument의 양식(제목, 소제목, 글꼴, 문단모양)을 이해하고, 최대한 준수한다.
1. 내용을 작성할 때에는 문단의 직전, 직후 문단에 기재된 제목/소제목(문서 양식마다 1~4개의 레벨이 존재한다)에 상응하도록 내용을 작성한다.
1. 별도의 GUI를 통해 제공되는 파일(hwp 등, Retrieval-Augmented Generation)들에 기재된 사실/근거들을 바탕으로, 사용자가 원하는 내용을 제시한다.
1. 양식 준수의 기준 우선순위
    1. 줄 수
    1. 줄간격, 문단여백
    1. 자간
1. 사용자의 스타일을 학습하여 출력을 최적화하는 기능
   * 제목/소제목 문단을 수정/완성할 때에는 직후 문단을 참고하여 1줄 이내로 입력 또는 수정한다.
   * 별도 모드가 아니라 커서의 컨텍스트가 제목/소제목/본문이라는 정보를 프롬프트에 추가하는 기능
1. LLM Local or remote Server(nvidia spark dgx 등)를 지원
1. Claude api, Gemini api는 추후지원으로 일단 설계에만 반영

## Workflow
1. 사용자가 문단에 프롬프트 입력(생략가능) 
1. 사용자가 eat_hwp_raw 호출하는 addon의 `버튼1` 클릭 or 단축키 `Ctrl+]`
1. eat_hwp_raw GUI 팝업(별도 프로세스)
1. eat_hwp_raw는 hwp_automation_rs로 현재 컨텍스트(커서위치, 블록 정보 등) 확보
1. SaveAs로 TMP 디렉토리에 hwpx 저장 후 hwp사용자는 기존 문서 작업 계속, 또는 별개의 Workflow 실행 가능
1. eat_hwp_raw GUI에 자유롭게 프롬프트 입력 
1. eat_hwp_raw는 TMP에 저장된 hwpx를 [hwpers](https://github.com/Indosaram/hwpers)로
   파싱하고, 프롬프트 실행
1. 결과물은 별도의 새 hwp 창을 띄워서 거기에 입력하되, 그 결과물에 맞는 서식을
   최대한 적용
1. 처리완료 알람 팝업(windows api)

## eat_hwp_raw GUI
1. 현재문단/전체문서를 컨텍스트로 지정할 수 있는 check button
1. 프롬프트/답변 textarea(답변 textarea는 새 hwp에 창에 입력된 내용과 동일)
1. 프롬프트 commit 버튼
1. etc

## 개발환경 llm server (WSL2, RTX 5070)
1. 외부 llm service
1. local or intranet llm server(gemma4 or etc)
1. windows -> wsl2(localhost) 접속은 설정완료(ex: windows chrome browser에서
    wsl2에서 실행한 웹서버 localhost:8000 으로 접속가능)
1. vram 12GB, RTX5070

### 서버 구성 (개발용: RTX 5070 12GB)
- **용도**: WSL2는 개발환경이다. 실제 실행환경은 아래 "실행환경 llm server"이며, 최종 검증은
  실행환경 모델(`gemma4:e4b-it-qat`)로 한다 — 이 서버에 함께 pull해 두면 모델 env만 바꿔 확인 가능.
- **서빙: Ollama** — 포트 `11434`, OpenAI 호환 `/v1`. 단일 사용자 GUI엔 충분하고 GGUF
  양자화로 12GB에 적합. (대규모 동시성/원격 DGX로 확장 시 vLLM `:8000/v1`로 교체 — 앱은
  base URL env만 변경.)
- **모델: Gemma 4 12B dense, `Q4_K_M`** — 가중치 ~7–8GB로 12GB에 KV 캐시 여유. 26B A4B(MoE)는
  Q4 ~15GB로 12GB 초과(CPU 스필) → 제외. E4B는 양식 준수가 약해 폴백용.
- **컨텍스트: `num_ctx` 16K–32K** — 256K 풀은 KV 캐시가 VRAM을 초과. 문서+RAG를 커버하는 실용값.
- **드라이버/주의**: RTX 5070은 Blackwell(sm_120) → Windows에 최신 NVIDIA WSL 지원 드라이버
  (R570+) + 최신 Ollama 빌드 필수(구버전은 GPU 미인식). WSL 내부엔 Linux NVIDIA 드라이버 설치 금지.
- **`.wslconfig`**(`%UserProfile%\.wslconfig`): `[wsl2]` `memory=24GB`, `processors=<대부분>`,
  `swap=8GB`, `networkingMode=mirrored`(localhost 접속 보장, 이미 동작). vhdx 여유 60GB+.

#### 구동
```bash
# WSL2(Ubuntu 24.04) 안에서
curl -fsSL https://ollama.com/install.sh | sh        # 최신 = Blackwell 지원
ollama --version
ollama pull gemma4:12b                                 # 정확 태그는 `ollama list`로 확인
# (라이브러리에 없으면) HF GGUF를 import:
#   printf 'FROM ./gemma4-12b-Q4_K_M.gguf\nPARAMETER num_ctx 32768\n' > Modelfile
#   ollama create gemma4:12b -f Modelfile
ollama run gemma4:12b "한국어로 인사해줘"             # GPU 로드 + 한국어 확인
```

#### eat_hwp_raw 연결 (env, Windows 측)
- `EAT_HWP_LLM_BASE_URL` = `http://localhost:11434/v1` (= 기본값, mirrored라 그대로 닿음)
- `EAT_HWP_LLM_MODEL` = `gemma4:12b` (기본값은 실행환경 모델 `gemma4:e4b-it-qat`이므로 12B를 쓸 때만
  지정. `ollama list` 태그와 정확히 일치)
- `EAT_HWP_LLM_API_KEY` = (빈 값, Ollama는 불필요)

## 실행환경 llm server (Windows 10/11, Intel 내장 그래픽)
1. 실제 실행환경: windows 10/11, intel 내장 그래픽, minimum ram 16GiB
1. intel 내장 그래픽 구동 가능한 llm

### 서버 구성 (Intel 내장 그래픽, 최소 RAM 16GiB)
- **용도**: 실제 사용자 PC 구성. 앱 기본값과 양식 준수 품질 평가는 이 구성을 기준으로 한다.
- **서빙: Ollama (Windows 네이티브)** — 포트 `11434`, OpenAI 호환 `/v1`. 한글·eat_hwp_raw와
  같은 Windows에서 직접 실행하므로 WSL2가 필요 없다.
- **모델: 기본 `gemma4:e4b-it-qat`(6.1GB)** — RAM 16GiB를 Windows·한글·내장 그래픽 공유 메모리와
  나눠 쓰므로 가중치 ~6GB가 상한. 후보와 선정 기준은 아래 "모델 후보" 참조(실기 측정 전 잠정).
- **컨텍스트: `num_ctx` 8K** — OpenAI 호환 API로는 요청별 지정이 안 되므로 서버 env
  `OLLAMA_CONTEXT_LENGTH=8192`로 설정. 전체문서 컨텍스트는 초과하기 쉬우므로 현재문단 컨텍스트가 기본 사용 형태.
- **가속/주의**: **CPU 추론을 기준선으로 잡는다.** 내장 그래픽 가속은 Vulkan 백엔드 경유인데
  (Windows는 Intel 그래픽 드라이버에 Vulkan 포함), Ollama가 내장 그래픽을 기본으로 건너뛰어
  `OLLAMA_IGPU_ENABLE=1`이 필요하다는 보고(공식 문서엔 없음)와 Iris Xe 미인식·출력 깨짐·러너 크래시
  사례가 있다. 가속이 되면 덤이고, 증상이 있으면 `OLLAMA_VULKAN=0`으로 CPU 전환. 어느 쪽이든
  개발환경(RTX)보다 응답이 크게 느리므로 모델 크기가 곧 체감 속도다.

#### 모델 후보 (16GiB·CPU/내장 그래픽에서 구동 가능)
크기는 Ollama 태그 목록 기준, 벤치마크는 각 모델 카드의 자체 발표치다. **이 환경에서 직접 돌려본
결과가 아니므로** 실기에서 속도(tok/s)와 양식 준수를 측정해 확정한다.

| 후보 | 크기 | 라이선스 | 구동 | 비고 |
|---|---|---|---|---|
| Gemma 4 E4B QAT | 6.1GB | 확인 필요 | `gemma4:e4b-it-qat` | 현 기본값. 후보 중 가장 크고 느림. 컨텍스트 128K |
| Gemma 4 E2B QAT | 4.3GB | 확인 필요 | `gemma4:e2b-it-qat` | E4B가 느리거나 메모리 부족일 때 |
| Mi:dm 2.0 Mini (KT, 2.3B) | Q4 GGUF(소형) | MIT | 커뮤니티 GGUF `hf.co/mykor/Midm-2.0-Mini-Instruct-gguf` | 한국어 특화. 모델 카드상 한국어 지시 이행 73.6로 Qwen3-4B(69.4)·EXAONE 3.5 2.4B(68.9)보다 높음. 공식 GGUF는 Base만 제공 |
| Kanana 1.5 2.1B (Kakao) | Q4 GGUF(소형) | Apache-2.0 | 커뮤니티 GGUF(HF) | 한국어 특화. 컨텍스트 32K 기본 |
| Qwen3 4B / 8B | 2.5GB / 5.2GB | 확인 필요 | `qwen3:4b`, `qwen3:8b` | 다국어 범용. Ollama 공식 라이브러리 |
| EXAONE 3.5 2.4B / 7.8B (LG) | 1.6GB / 4.8GB | **비상업(NC)** | `exaone3.5:2.4b`, `exaone3.5:7.8b` | 한국어 품질은 좋으나 연구·비상업 용도 한정 → 배포용 기본값으로 부적합 |

- **제외**: Gemma 4 12B(7.2–8.0GB)·Mi:dm 2.0 Base(11.5B)는 16GiB에서 스왑 위험, 26B 이상·EXAONE 32B·
  HyperCLOVA X SEED 14B/32B는 메모리 초과.
- **선정 순서(제안)**: ① `gemma4:e4b-it-qat`가 실기에서 쓸 만한 속도면 유지 → ② 느리면 한국어 특화
  소형인 Mi:dm 2.0 Mini(MIT)와 `gemma4:e2b-it-qat`를 같은 문서로 비교 → ③ 그래도 부족하면 `qwen3:4b`.
  어느 것이든 앱은 `EAT_HWP_LLM_MODEL`만 바꾸면 된다.

#### 구동
```powershell
# Windows(PowerShell). Ollama for Windows 최신 버전 설치 후
setx OLLAMA_CONTEXT_LENGTH 8192                        # 설정 후 Ollama 재시작(트레이 종료 → 재실행)
ollama pull gemma4:e4b-it-qat
ollama run gemma4:e4b-it-qat "한국어로 인사해줘"       # 한국어 출력 확인
ollama ps                                              # PROCESSOR 열로 GPU/CPU 로드 확인
ollama run gemma4:e4b-it-qat --verbose "한 문단 써줘"   # eval rate(tok/s) 측정 → 모델 선정 근거
# 출력이 깨지거나 크래시하면: setx OLLAMA_VULKAN 0 후 Ollama 재시작(CPU 추론)
```

#### eat_hwp_raw 연결 (env, Windows 측)
- `EAT_HWP_LLM_BASE_URL` = `http://localhost:11434/v1` (= 기본값)
- `EAT_HWP_LLM_MODEL` = `gemma4:e4b-it-qat` (= 기본값. E2B 폴백 시에만 `gemma4:e2b-it-qat` 지정)
- `EAT_HWP_LLM_API_KEY` = (빈 값)
