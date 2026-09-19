import { joinSegments, pathToRoot } from "../util/path"
import { QuartzComponent, QuartzComponentConstructor, QuartzComponentProps } from "./types"
import { classNames } from "../util/lang"
import { i18n } from "../i18n"

const PageTitle: QuartzComponent = ({ fileData, cfg, displayClass }: QuartzComponentProps) => {
  const title = cfg?.pageTitle ?? i18n(cfg.locale).propertyDefaults.title
  const baseDir = pathToRoot(fileData.slug!)
  // Moonkale: the app icon next to the title (static/icon.png, from /assets/Moonkale512.png).
  const icon = joinSegments(baseDir, "static/icon.png")
  return (
    <h2 class={classNames(displayClass, "page-title")}>
      <a href={baseDir}>
        <img class="page-title-icon" src={icon} alt="" width="28" height="28" />
        {title}
      </a>
    </h2>
  )
}

PageTitle.css = `
.page-title {
  font-size: 1.75rem;
  margin: 0;
  font-family: var(--titleFont);
}
.page-title a { display: inline-flex; align-items: center; gap: 0.5rem; }
.page-title-icon { width: 28px; height: 28px; border-radius: 6px; }
`

export default (() => PageTitle) satisfies QuartzComponentConstructor
