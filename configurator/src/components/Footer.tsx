import { Icon } from "./Icon";

export const Footer = () => (
  <footer className="border-default-100 mt-16 border-t-3">
    <div className="flex items-center justify-between py-8">
      <a href="https://atov.de" target="_blank">
        <img src="/img/atov-logo.svg" className="w-16" />
      </a>
      <div className="flex items-center gap-4">
        <a href="https://atov.de/discord" target="_blank">
          <Icon className="h-6 w-6" name="discord" />
        </a>
        <a href="https://github.com/ATOVproject/faderpunk" target="_blank">
          <Icon className="h-6 w-6" name="github" />
        </a>
        <a href="https://www.instagram.com/atovproject/" target="_blank">
          <Icon className="h-6 w-6" name="instagram" />
        </a>
      </div>
    </div>
  </footer>
);
