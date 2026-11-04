import { useEffect, useRef, useState } from "react";
import type { Application } from "pixi.js";
import type { Cubism4InternalModel, Live2DModel } from "pixi-live2d-display/cubism4";

export type PetLoadStatus = "loading" | "ready" | "error";
type Props = {
  paused: boolean;
  resting: boolean;
  poked: boolean;
  onStatusChange: (status: PetLoadStatus) => void;
};

export function Live2DPet({ paused, resting, poked, onStatusChange }: Props) {
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
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    const report = (next: PetLoadStatus) => { setStatus(next); onStatusChange(next); };
    const release = () => {
      playback.current = null;
      observer?.disconnect();
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
      const { Live2DModel } = await import("pixi-live2d-display/cubism4");
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

      const loaded = await Live2DModel.from("/live2d/atri/model.json", {
        autoUpdate: false,
        autoInteract: false,
      }) as Live2DModel<Cubism4InternalModel>;
      if (disposed || failed) {
        loaded.destroy({ children: true, texture: true, baseTexture: true });
        return;
      }
      model = loaded;
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
        syncPlayback();
      };
      fit();
      observer = new ResizeObserver(fit);
      observer.observe(host);
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
  }, [onStatusChange]);

  return <>
    <span className="live2d-pet" ref={hostRef} aria-hidden="true" />
    {status !== "ready" && <span className="pet-load-status" role={status === "error" ? "alert" : "status"}>
      <span aria-hidden="true">✳</span>
      {status === "error" ? "角色加载失败" : "角色加载中…"}
      <small>{status === "error" ? "可重试，或点击这里打开菜单" : "正在读取本地模型"}</small>
    </span>}
  </>;
}
