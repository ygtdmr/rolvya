import { SECTIONS, isId, type Section } from "../api/model";
export interface Route {
  section: Section;
  project: string | null;
  task: string | null;
}
export function parseRoute(hash: string): Route {
  const [path, query = ""] = hash.replace(/^#/, "").split("?");
  const [section, project] = (path ?? "").replace(/^\//, "").split("/");
  const params = new URLSearchParams(query);
  const task = params.get("task");
  return {
    section: SECTIONS.includes(section as Section)
      ? (section as Section)
      : "projects",
    project: project && isId(project) ? project : null,
    task: task && isId(task) ? task : null,
  };
}
export function routeHash(route: Route): string {
  return `#/${route.section}${route.project && isId(route.project) ? "/" + route.project : ""}${route.task && isId(route.task) ? "?task=" + encodeURIComponent(route.task) : ""}`;
}
