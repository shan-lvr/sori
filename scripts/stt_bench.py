"""On-device STT benchmark vs ElevenLabs Scribe v2 (the current cloud engine).

  uv run --with sherpa-onnx --with numpy --with soundfile --with jiwer python scripts/stt_bench.py <engine> [<engine> ...]

Engines: elevenlabs | whisper:<ggml file> | sense_voice | moonshine_ko | qwen3 | funasr_nano | cohere
Data (see .scratch/stt/data): FLEURS ko (40 clips, CER), FLEURS en (15 clips, WER), dev/ (8 synthetic Korean
dev-jargon clips, printed for eyeballing). Results are appended to .scratch/stt/results.jsonl.
"""
import glob, json, os, re, subprocess, sys, time, unicodedata, urllib.request, uuid
import numpy as np, soundfile as sf, jiwer

ROOT = os.path.join(os.path.dirname(__file__), "..", ".scratch", "stt")
M = os.path.join(ROOT, "models")
THREADS = 8
DEV_HOT = "React Query, useEffect, git rebase, force push, pull request, Docker Compose, PostgreSQL, Redis, .env, Claude Code, Tauri, CGEventTap, Next.js, App Router, Server Actions, Zod"

def norm_ko(s):
    s = unicodedata.normalize("NFKC", s).lower()
    return re.sub(r"[\s\W_]+", "", s)

def norm_en(s):
    s = unicodedata.normalize("NFKC", s).lower()
    s = re.sub(r"[^\w\s']", " ", s)
    return re.sub(r"\s+", " ", s).strip()

def sherpa_rec(kind, lang):
    import sherpa_onnx as so
    d = lambda pat: glob.glob(os.path.join(M, pat))[0]
    if kind == "sense_voice":
        b = d("sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09")
        return so.OfflineRecognizer.from_sense_voice(model=f"{b}/model.int8.onnx", tokens=f"{b}/tokens.txt", num_threads=THREADS, language=lang, use_itn=True)
    if kind == "moonshine_ko":
        b = d("sherpa-onnx-moonshine-tiny-ko-quantized-*")
        enc = glob.glob(f"{b}/*encoder*")[0]; dec = glob.glob(f"{b}/*decoder*")[0]
        return so.OfflineRecognizer.from_moonshine_v2(encoder=enc, decoder=dec, tokens=f"{b}/tokens.txt", num_threads=THREADS)
    if kind == "qwen3":
        b = d("sherpa-onnx-qwen3-asr-0.6B-int8-*")
        f = lambda p: glob.glob(f"{b}/{p}")[0]
        return so.OfflineRecognizer.from_qwen3_asr(conv_frontend=f("conv_frontend*.onnx"), encoder=f("encoder*.onnx"), decoder=f("decoder*.onnx"),
                                                   tokenizer=b if not glob.glob(f"{b}/tokenizer*") else (glob.glob(f"{b}/tokenizer")[0] if os.path.isdir(f"{b}/tokenizer") else b),
                                                   num_threads=THREADS, max_new_tokens=512, max_total_len=1024,
                                                   hotwords=os.environ.get("HOTWORDS", ""))
    if kind == "funasr_nano":
        b = d("sherpa-onnx-funasr-nano-int8-*")
        f = lambda p: glob.glob(f"{b}/{p}")[0]
        return so.OfflineRecognizer.from_funasr_nano(encoder_adaptor=f("encoder_adaptor*.onnx"), llm=f("llm*.onnx"), embedding=f("embedding*.onnx"),
                                                     tokenizer=f("Qwen*") if glob.glob(f"{b}/Qwen*") else b, num_threads=THREADS)
    if kind == "cohere":
        b = d("sherpa-onnx-cohere-transcribe-*")
        return so.OfflineRecognizer.from_cohere_transcribe(encoder=f"{b}/encoder.int8.onnx", decoder=f"{b}/decoder.int8.onnx",
                                                           tokens=f"{b}/tokens.txt", num_threads=THREADS, language=lang)
    raise SystemExit(f"unknown sherpa engine {kind}")

class Engine:
    def __init__(self, spec):
        self.spec, self.kind = spec, spec.split(":")[0]
        self.recs, self.server = {}, None
        if self.kind == "whisper":
            self.port = 8910
            self.server = subprocess.Popen(["whisper-server", "-m", os.path.join(M, spec.split(":", 1)[1]), "--port", str(self.port), "-t", str(THREADS), "-nt", "-l", "auto"],
                                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            for _ in range(120):
                try: urllib.request.urlopen(f"http://127.0.0.1:{self.port}/", timeout=1); break
                except Exception: time.sleep(0.5)

    def close(self):
        if self.server: self.server.terminate()

    def transcribe(self, path, lang):
        if self.kind == "elevenlabs":
            key = [l.split("=", 1)[1].strip() for l in open(os.path.join(ROOT, "..", "..", ".env.local")) if l.startswith("ELEVENLABS")][0]
            b = "----" + uuid.uuid4().hex
            parts = [(f'--{b}\r\nContent-Disposition: form-data; name="model_id"\r\n\r\nscribe_v2\r\n').encode(),
                     (f'--{b}\r\nContent-Disposition: form-data; name="language_code"\r\n\r\n{lang}\r\n').encode(),
                     (f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="a.wav"\r\nContent-Type: audio/wav\r\n\r\n').encode() + open(path, "rb").read() + b"\r\n",
                     f"--{b}--\r\n".encode()]
            req = urllib.request.Request("https://api.elevenlabs.io/v1/speech-to-text", data=b"".join(parts),
                                         headers={"xi-api-key": key, "Content-Type": f"multipart/form-data; boundary={b}"})
            return json.load(urllib.request.urlopen(req, timeout=120)).get("text", "")
        if self.kind == "whisper":
            b = "----" + uuid.uuid4().hex
            body = (f'--{b}\r\nContent-Disposition: form-data; name="language"\r\n\r\n{lang}\r\n'
                    f'--{b}\r\nContent-Disposition: form-data; name="response_format"\r\n\r\njson\r\n'
                    f'--{b}\r\nContent-Disposition: form-data; name="file"; filename="a.wav"\r\nContent-Type: audio/wav\r\n\r\n').encode() + open(path, "rb").read() + f"\r\n--{b}--\r\n".encode()
            req = urllib.request.Request(f"http://127.0.0.1:{self.port}/inference", data=body, headers={"Content-Type": f"multipart/form-data; boundary={b}"})
            return json.load(urllib.request.urlopen(req, timeout=300)).get("text", "").strip()
        rec = self.recs.get(lang) or self.recs.setdefault(lang, sherpa_rec(self.kind, lang))
        audio, sr = sf.read(path, dtype="float32")
        st = rec.create_stream(); st.accept_waveform(sr, audio); rec.decode_stream(st)
        return st.result.text.strip()

def run(spec):
    eng = Engine(spec)
    if os.environ.get("HOTWORDS"): spec_label = spec + "+hotwords"
    else: spec_label = spec
    res = {"engine": spec_label}
    try:
        for set_name, lang, metric in [("fleurs_ko", "ko", "cer"), ("fleurs_en", "en", "wer"), ("dev", "ko", None)]:
            if set_name == "dev":
                items = [{"file": f, "ref": ""} for f in sorted(glob.glob(os.path.join(ROOT, "data/dev/*.wav")))]
            else:
                items = json.load(open(os.path.join(ROOT, f"data/{set_name}.json")))
                for it in items: it["file"] = os.path.join(ROOT, it["file"])
            eng.transcribe(items[0]["file"], lang)  # warm-up (model load)
            hyps, times, secs = [], [], 0.0
            for it in items:
                t = time.time(); h = eng.transcribe(it["file"], lang); times.append(time.time() - t)
                hyps.append(h); secs += sf.info(it["file"]).duration
            r = {"clips": len(items), "audio_s": round(secs, 1), "avg_latency_s": round(float(np.mean(times)), 3), "p90_latency_s": round(float(np.percentile(times, 90)), 3), "rtf": round(sum(times) / secs, 4)}
            if metric == "cer":
                r["cer"] = round(jiwer.cer([norm_ko(i["ref"]) for i in items], [norm_ko(h) or "∅" for h in hyps]), 4)
            elif metric == "wer":
                r["wer"] = round(jiwer.wer([norm_en(i["ref"]) for i in items], [norm_en(h) or "∅" for h in hyps]), 4)
            else:
                r["outputs"] = hyps
            res[set_name] = r
            print(f"{spec_label:40s} {set_name:10s} " + " ".join(f"{k}={v}" for k, v in r.items() if k != "outputs"), flush=True)
            if set_name == "dev":
                for h in hyps: print("   ", h)
    finally:
        eng.close()
    with open(os.path.join(ROOT, "results.jsonl"), "a") as f:
        f.write(json.dumps(res, ensure_ascii=False) + "\n")

if __name__ == "__main__":
    for spec in sys.argv[1:]:
        try: run(spec)
        except Exception as e: print(f"{spec}: FAILED {e!r}", flush=True)
