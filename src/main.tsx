import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { native } from "./core";
import { PetWindow } from "./PetWindow";
import { BubbleWindow } from "./BubbleWindow";

const label = native ? getCurrentWindow().label : "main";
const Root = label === "pet" ? PetWindow : label === "bubble" ? BubbleWindow : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
