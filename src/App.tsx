import { useEffect, useRef, useState } from "react";
import "./App.css";

export default function App() {
  const hostRef = useRef<HTMLDivElement>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let app: import("pixi.js").Application | undefined;
    let observer: ResizeObserver | undefined;

    const boot = async () => {
      const PIXI = await import("pixi.js");
      const { Live2DModel } = await import("pixi-live2d-display/cubism4");
      if (disposed) return;

      Live2DModel.registerTicker(PIXI.Ticker);

      const canvas = document.createElement("canvas");
      app = new PIXI.Application({
        view: canvas,
        width: host.clientWidth,
        height: host.clientHeight,
        transparent: true,
        backgroundAlpha: 0,
        antialias: true,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
      });
      if (disposed) {
        app.destroy(true, { children: true, texture: true, baseTexture: true });
        app = undefined;
        return;
      }
      host.appendChild(canvas);

      const model = await Live2DModel.from("/live2d/atri/model.json");
      if (disposed) {
        model.destroy();
        return;
      }
      app.stage.addChild(model);

      const fit = () => {
        if (!app) return;
        const width = host.clientWidth;
        const height = host.clientHeight;
        app.renderer.resize(width, height);
        const baseWidth = model.internalModel.width || model.width / model.scale.x;
        const baseHeight = model.internalModel.height || model.height / model.scale.y;
        const scale = Math.min(width / baseWidth, height / baseHeight) * 0.92;
        model.scale.set(scale);
        model.anchor.set(0.5, 0.5);
        model.position.set(width / 2, height / 2);
      };
      fit();
      observer = new ResizeObserver(fit);
      observer.observe(host);
      void model.motion("hair", 0);
    };

    boot().catch((err: unknown) => {
      if (!disposed) setError(err instanceof Error ? err.message : String(err));
    });

    return () => {
      disposed = true;
      observer?.disconnect();
      app?.destroy(true, { children: true, texture: true, baseTexture: true });
    };
  }, []);

  return (
    <main className="stage">
      <div className="live2d" ref={hostRef} />
      {error ? <p className="live2d-error">{error}</p> : null}
    </main>
  );
}
