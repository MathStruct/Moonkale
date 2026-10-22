import { siteLayout } from "./mathstruct/layout"

export const { sharedPageComponents, defaultContentPageLayout, defaultListPageLayout } =
  siteLayout({
    footerLinks: {
      GitHub: "https://github.com/MathStruct/Moonkale",
      MathStruct: "https://mathstruct.org/",
    },
    // the app icon next to the title (site/static/icon.png, from assets/Moonkale512.png)
    icon: "static/icon.png",
    // a giscus comment thread on every page (GitHub Discussions, category Announcements)
    comments: {
      repo: "MathStruct/Moonkale",
      repoId: "R_kgDOUeDr7g",
      category: "Announcements",
      categoryId: "DIC_kwDOUeDr7s4DJXDC",
    },
  })
