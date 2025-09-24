export const About = () => (
  <div className="text-lg/10">
    <p className="mb-12">
      Faderpunk is a production by{" "}
      <a className="underline" href="https://atov.de" target="_blank">
        ATOV
      </a>
      . Go to{" "}
      <a className="underline" href="https://faderpunk.cv">
        https://faderpunk.cv
      </a>{" "}
      for more info.
    </p>

    <h2 className="text-yellow-fp mb-4 text-sm font-bold uppercase">
      Acknowledgements and attributions
    </h2>
    <ul>
      <li className="mb-2 flex items-start">
        <span className="w-8">💫</span>
        <span>
          UI and UX by the wonderful and extraordinarily talented{" "}
          <a
            className="underline"
            href="https://github.com/estcla"
            target="_blank"
          >
            Leise St. Clair
          </a>{" "}
          (seriously, what would have happened here without you??)
        </span>
      </li>
      <li className="mb-2 flex items-start">
        <span className="w-8">🎛️</span>
        <span>
          Icon design by{" "}
          <a
            className="underline"
            href="https://www.papernoise.net"
            target="_blank"
          >
            papernoise
          </a>
        </span>
      </li>
      <li className="mb-2 flex items-start">
        <span className="w-8">📐</span>
        <span>
          log, lin and exp icons by{" "}
          <a
            className="underline"
            href="https://thenounproject.com/creator/declerckrobbe/"
            target="_blank"
          >
            Robbe de Clerck
          </a>
        </span>
      </li>
      <li className="mb-2 flex items-start">
        <span className="w-8">🎹</span>
        <span>
          MIDI icon by{" "}
          <a
            className="underline"
            href="https://www.onlinewebfonts.com/icon/496520"
            target="_blank"
          >
            onlinewebfonts
          </a>
        </span>
      </li>
    </ul>
    <a
      href="https://atov.de/about"
      className="text-default-400 mt-4 cursor-pointer underline"
    >
      Imprint
    </a>
  </div>
);
