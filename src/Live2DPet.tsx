import { useEffect, useRef, useState } from "react";
import type { Application } from "pixi.js";
import type { Cubism4InternalModel, Live2DModel } from "pixi-live2d-display/cubism4";
import type { ModelLayout } from "./core";

export type PetLoadStatus = "loading" | "ready" | "error";
type Props = {
  paused: boolean;
  resting: boolean;
  poked: boolean;
  onStatusChange: (status: PetLoadStatus) => void;
  onLayout: (layout: ModelLayout) => void;
};

export function Live2DPet({ paused, resting, poked, onStatusChange, onLayout }: Props) {
  const hostRef = useRef<HTMLSpanElement>(null);
  const playback = useRef<(() => void) | null>(null);
  const state = useRef({ paused, resting, poked });
  const [status, setStatus] = useState<PetLoadStatus>("loading");

  useEffect(() => {
    state.current = { paused, resting, poked };
    playback.current?.();
  }, [paused, resting, poked]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let failed = false;
    let app: Application | undefined;
    let model: Live2DModel<Cubism4InternalModel> | undefined;
    let observer: ResizeObserver | undefined;
    let densityQuery: MediaQueryList | undefined;
    let onDensityChange: (() => void) | undefined;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const report = (next: PetLoadStatus) => { setStatus(next); onStatusChange(next); };
    const release = () => {
      playback.current = null;
      observer?.disconnect();
      if (densityQuery && onDensityChange) densityQuery.removeEventListener("change", onDensityChange);
      app?.destroy(true, { children: true, texture: true, baseTexture: true });
      app = undefined;
      model = undefined;
    };
    const fail = (error: unknown) => {
      if (disposed || failed) return;
      failed = true;
      clearTimeout(timeout);
      console.error("Live2D 加载失败", error);
      release();
      report("error");
    };
    // A stalled request must not leave an invisible or permanently loading pet.
    const timeout = window.setTimeout(() => fail(new Error("模型加载超时")), 20_000);
    const syncPlayback = () => {
      if (!app || !model || disposed || failed) return;
      const stop = state.current.paused || reducedMotion.matches || document.hidden;
      app.stop();
      // Render once while paused so sleep/wake and resizing remain visible.
      model.update(1);
      app.render();
      if (!stop) app.start();
    };
    const onVisibilityChange = () => syncPlayback();
    document.addEventListener("visibilitychange", onVisibilityChange);
    reducedMotion.addEventListener("change", onVisibilityChange);
    report("loading");

    const boot = async () => {
      const PIXI = await import("pixi.js");
      const { install } = await import("@pixi/unsafe-eval");
      // Replaces generated shader functions; CSP still forbids eval.
      install(PIXI);
      const { Live2DModel, Live2DLoader } = await import("pixi-live2d-display/cubism4");
      if (disposed || failed) return;

      app = new PIXI.Application({
        width: host.clientWidth,
        height: host.clientHeight,
        backgroundAlpha: 0,
        antialias: true,
        autoDensity: true,
        autoStart: false,
        sharedTicker: false,
        resolution: window.devicePixelRatio || 1,
      });
      app.ticker.maxFPS = 30;
      host.appendChild(app.view);

      let modelId = "";
      // Hash the already-loaded moc bytes: replacing a model at the same path
      // must not reuse the previous character's calibration.
      const identify: typeof Live2DLoader.middlewares[number] = async (context, next) => {
        await next();
        if (context.type === "arraybuffer" && context.settings && context.url === context.settings.moc) {
          const hash = await crypto.subtle.digest("SHA-256", context.result);
          modelId = Array.from(new Uint8Array(hash), (byte) => byte.toString(16).padStart(2, "0")).join("");
        }
      };
      Live2DLoader.middlewares.unshift(identify);
      let loaded: Live2DModel<Cubism4InternalModel>;
      try {
        loaded = await Live2DModel.from("/live2d/atri/model.json", { autoUpdate: false, autoInteract: false }) as Live2DModel<Cubism4InternalModel>;
      } finally {
        const index = Live2DLoader.middlewares.indexOf(identify);
        if (index >= 0) Live2DLoader.middlewares.splice(index, 1);
      }
      if (!modelId) { loaded.destroy({ children: true, texture: true, baseTexture: true }); throw new Error("无法识别角色模型"); }
      if (disposed || failed) {
        loaded.destroy({ children: true, texture: true, baseTexture: true });
        return;
      }
      model = loaded;
      const head = Object.values(model.internalModel.hitAreas).find((area) => /^(head|face|hitareahead|hitareaface)$/i.test(area.name) || /^(head|face|hitareahead|hitareaface)$/i.test(area.id));
      let defaultAnchor: ModelLayout["defaultAnchor"] = null;
      if (head) {
        const bounds = model.internalModel.getDrawableBounds(head.index);
        const centre = model.internalModel.localTransform.apply({ x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 });
        const point = { x: centre.x / model.internalModel.width, y: centre.y / model.internalModel.height };
        if ([point.x, point.y].every((v) => Number.isFinite(v) && v >= 0 && v <= 1)) defaultAnchor = point;
      }
      app.stage.addChild(model);
      // One ticker owns both rendering and animation, including physics.
      app.ticker.add(() => model?.update(app!.ticker.deltaMS), undefined, PIXI.UPDATE_PRIORITY.HIGH);
      model.internalModel.on("beforeModelUpdate", () => {
        const core = loaded.internalModel.coreModel;
        if (state.current.resting) {
          core.setParameterValueById("ParamEyeLOpen", 0);
          core.setParameterValueById("ParamEyeROpen", 0);
          core.setParameterValueById("ParamMouthOpenY", 0);
        } else if (!state.current.paused && !reducedMotion.matches && state.current.poked) {
          core.setParameterValueById("ParamCheek", 1);
          core.setParameterValueById("ParamEyeLSmile", 1);
          core.setParameterValueById("ParamEyeRSmile", 1);
          core.setParameterValueById("ParamMouthForm", 1);
          core.addParameterValueById("ParamAngleZ", -8);
        }
      });
      const fit = () => {
        if (!app || !model) return;
        const width = host.clientWidth;
        const height = host.clientHeight;
        app.renderer.resolution = window.devicePixelRatio || 1;
        app.renderer.resize(width, height);
        const scale = Math.min(width / model.internalModel.width, height / model.internalModel.height) * 0.92;
        model.scale.set(scale);
        model.anchor.set(0.5, 0.5);
        model.position.set(width / 2, height / 2);
        const fitWidth = model.internalModel.width * scale / width;
        const fitHeight = model.internalModel.height * scale / height;
        onLayout({ modelId, x: (1 - fitWidth) / 2, y: (1 - fitHeight) / 2, width: fitWidth, height: fitHeight, defaultAnchor });
        syncPlayback();
      };
      fit();
      observer = new ResizeObserver(fit);
      observer.observe(host);
      const watchDensity = () => {
        densityQuery = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
        densityQuery.addEventListener("change", onDensityChange!, { once: true });
      };
      onDensityChange = () => { fit(); watchDensity(); };
      watchDensity();
      playback.current = syncPlayback;
      clearTimeout(timeout);
      report("ready");
    };
    void boot().catch(fail);

    return () => {
      disposed = true;
      clearTimeout(timeout);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      reducedMotion.removeEventListener("change", onVisibilityChange);
      release();
    };
  }, [onStatusChange, onLayout]);

  return <>
    <span className="live2d-pet" ref={hostRef} aria-hidden="true" />
    {status !== "ready" && <span className="pet-load-status" role={status === "error" ? "alert" : "status"}>
      <span aria-hidden="true">✳</span>
      {status === "error" ? "角色加载失败" : "角色加载中…"}
      <small>{status === "error" ? "可重试，或点击这里打开菜单" : "正在读取本地模型"}</small>
    </span>}
  </>;
}
