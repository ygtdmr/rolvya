import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { flushSync } from "react-dom";
import { transitionView } from "../platform/motion";
import type {
  Availability,
  ProjectSummary as ProjectInfo,
  Section,
  Snapshot,
} from "../api/model";
import { SECTIONS } from "../api/model";
import { controllerSource, ReadError } from "../api/client";
import {
  coalesceInvalidations,
  EventOrder,
  reconnectDelay,
  streamProject,
} from "../api/events";
import { LocaleProvider, useLocale } from "../i18n/locale";
import { Projects } from "../features/projects";
import { Execution } from "../features/execution";
import { Costs } from "../features/costs";
import { Connections } from "../features/connections";
import { Results } from "../features/results";
import {
  Button,
  Icon,
  IconButton,
  Select,
  Switch,
  type IconName,
} from "../ui/controls";
import {
  Badge,
  ConnectivityBanner,
  EmptyState,
  ErrorState,
  PageHeader,
  Skeleton,
} from "../ui/data";
import { Drawer, Menu, Popover, Toast } from "../ui/overlays";
import { ProjectSelector, TaskInspector } from "../ui/patterns";
import { advanceDemo, demoSource } from "../testing/demo";
import { Gallery } from "../testing/gallery";
import { registerShell, type ShellUpdate } from "../platform/pwa";
import {
  loadPreferences,
  savePreferences,
  type Preferences,
} from "./preferences";
import { parseRoute, routeHash, type Route } from "./route";
const icons: Record<Section, IconName> = {
  projects: "projects",
  execution: "execution",
  "costs-approvals": "costs",
  connections: "connections",
  "results-activity": "results",
};
const source = __DEMO__ ? demoSource : controllerSource;
const subscribeRoute = (callback: () => void) => {
  window.addEventListener("hashchange", callback);
  return () => window.removeEventListener("hashchange", callback);
};
export function App() {
  const [prefs, setPrefs] = useState(loadPreferences);
  useEffect(() => {
    savePreferences(prefs);
    const root = document.documentElement;
    const dark = matchMedia("(prefers-color-scheme: dark)");
    const theme = () =>
      (root.dataset.theme =
        prefs.theme === "system"
          ? dark.matches
            ? "dark"
            : "light"
          : prefs.theme);
    theme();
    dark.addEventListener("change", theme);
    root.dataset.motion = prefs.motion;
    root.lang = prefs.locale.startsWith("qps-") ? "en" : prefs.locale;
    root.dir = prefs.locale === "qps-rtl" ? "rtl" : "ltr";
    return () => dark.removeEventListener("change", theme);
  }, [prefs]);
  return (
    <LocaleProvider locale={prefs.locale}>
      <Panel preferences={prefs} setPreferences={setPrefs} />
    </LocaleProvider>
  );
}
function Panel({
  preferences,
  setPreferences,
}: {
  preferences: Preferences;
  setPreferences: (p: Preferences) => void;
}) {
  const hash = useSyncExternalStore(subscribeRoute, () => location.hash);
  const route = parseRoute(hash);
  const { t } = useLocale();
  const [projects, setProjects] = useState<ProjectInfo[]>([]),
    [snapshot, setSnapshot] = useState<Snapshot | null>(null),
    [availability, setAvailability] = useState<Availability>("connecting"),
    [loading, setLoading] = useState(true),
    [refresh, setRefresh] = useState(0),
    [navOpen, setNavOpen] = useState(false),
    [inspector, setInspector] = useState(false),
    [gallery, setGallery] = useState(false),
    [toast, setToast] = useState(""),
    [update, setUpdate] = useState<ShellUpdate | null>(null),
    [hidden, setHidden] = useState(false);
  const navigate = useCallback((next: Route, replace = false) => {
    const target = routeHash(next);
    if (replace) {
      history.replaceState(null, "", target);
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    } else {
      const update = () =>
        flushSync(() => {
          history.pushState(null, "", target);
          window.dispatchEvent(new HashChangeEvent("hashchange"));
        });
      if (parseRoute(location.hash).section !== next.section)
        transitionView(update);
      else update();
    }
    setNavOpen(false);
  }, []);
  useEffect(() => {
    const c = new AbortController();
    setLoading(true);
    setSnapshot(null);
    setHidden(false);
    setAvailability(navigator.onLine ? "connecting" : "offline");
    void source
      .projects(c.signal)
      .then(async (list) => {
        if (c.signal.aborted) return;
        setProjects(list);
        const project =
          route.project && list.some((p) => p.id === route.project)
            ? route.project
            : list[0]?.id;
        if (route.project && !list.some((p) => p.id === route.project))
          throw new ReadError("forbidden");
        if (!project) {
          setAvailability("unavailable");
          return;
        }
        const next = await source.snapshot(project, c.signal);
        if (c.signal.aborted) return;
        setSnapshot(next);
        setAvailability(navigator.onLine ? "live" : "offline");
        if (!route.project) navigate({ ...route, project }, true);
      })
      .catch((e) => {
        if (c.signal.aborted) return;
        setSnapshot(null);
        setProjects([]);
        setAvailability(
          e instanceof ReadError
            ? e.kind === "invalid"
              ? "unavailable"
              : e.kind === "network"
                ? "offline"
                : e.kind
            : "unavailable",
        );
      })
      .finally(() => {
        if (!c.signal.aborted) setLoading(false);
      });
    return () => c.abort();
    // Section/task navigation preserves one project snapshot; only scope and refresh refetch it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [route.project, refresh, navigate]);
  useEffect(() => {
    if (!route.project || !snapshot?.capabilities.events || __DEMO__) return;
    const c = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    const order = new EventOrder(
      route.project,
      snapshot.cursor,
      snapshot.revision,
    );
    const invalidations = coalesceInvalidations(() => setRefresh((n) => n + 1));
    let retries = 0;
    const renew = async () => {
      if (c.signal.aborted) return;
      try {
        await streamProject(
          route.project!,
          snapshot.cursor,
          c.signal,
          (data, id) => {
            if (order.accept(data, id) !== "duplicate") {
              setAvailability("stale");
              invalidations.request();
            }
          },
        );
        if (!c.signal.aborted) setAvailability("stale");
      } catch (e) {
        if (c.signal.aborted) return;
        if (
          e instanceof ReadError &&
          (e.kind === "unauthorized" || e.kind === "forbidden")
        ) {
          invalidations.cancel();
          setSnapshot(null);
          setProjects([]);
          setAvailability(e.kind);
          return;
        }
        setAvailability(navigator.onLine ? "stale" : "offline");
      }
      if (!c.signal.aborted)
        timer = setTimeout(() => {
          void renew();
        }, reconnectDelay(retries++));
    };
    void renew();
    return () => {
      c.abort();
      clearTimeout(timer);
      invalidations.cancel();
    };
    // One stream belongs to one immutable snapshot/cursor; refreshing tears it down.
  }, [
    route.project,
    snapshot?.cursor,
    snapshot?.revision,
    snapshot?.capabilities.events,
  ]);
  useEffect(() => {
    const offline = () => setAvailability("offline");
    const online = () => {
      setAvailability("connecting");
      setRefresh((n) => n + 1);
    };
    window.addEventListener("offline", offline);
    window.addEventListener("online", online);
    return () => {
      window.removeEventListener("offline", offline);
      window.removeEventListener("online", online);
    };
  }, []);
  useEffect(() => registerShell(setUpdate), []);
  useEffect(() => {
    if (!toast) return;
    const timer = setTimeout(() => setToast(""), 6500);
    return () => clearTimeout(timer);
  }, [toast]);
  useEffect(() => {
    document
      .querySelector<HTMLElement>("[data-page-heading]")
      ?.focus({ preventScroll: true });
  }, [route.section]);
  const task = snapshot?.tasks.find((x) => x.id === route.task);
  const selectTask = (id: string) => {
    navigate({ ...route, task: id || null });
    setInspector(Boolean(id));
  };
  const chooseSection = (section: Section) => {
    navigate({ ...route, section });
    setInspector(false);
  };
  const navigation = (
    <nav aria-label="Primary navigation" className="rv-nav">
      {SECTIONS.map((section, index) => (
        <a
          key={section}
          href={routeHash({ ...route, section })}
          aria-current={route.section === section ? "page" : undefined}
          onClick={(e) => {
            if (
              !e.ctrlKey &&
              !e.metaKey &&
              !e.shiftKey &&
              !e.altKey &&
              e.button === 0
            ) {
              e.preventDefault();
              navigate({ ...route, section });
              setInspector(false);
            }
          }}
        >
          <Icon name={icons[section]} />
          <span>{t(section)}</span>
          <small>{String(index + 1).padStart(2, "0")}</small>
        </a>
      ))}
    </nav>
  );
  const project = snapshot?.project.name ?? "Select an authorized project";
  return (
    <div className="rv-app">
      <a
        className="rv-skip"
        href="#main"
        onClick={(e) => {
          e.preventDefault();
          document.getElementById("main")?.focus();
        }}
      >
        Skip to main content
      </a>
      <aside className="rv-sidebar">
        <a
          className="rv-wordmark"
          href={routeHash({ ...route, section: "projects" })}
        >
          Rolvya<span>CONNECTED CONTROL</span>
        </a>
        <div className="rv-workspace-label">
          <span>WORKSPACE</span>
          <Badge tone={__DEMO__ ? "caution" : "neutral"}>
            {__DEMO__ ? "DEMO" : "READ ONLY"}
          </Badge>
        </div>
        {navigation}
        <div className="rv-sidebar-bottom">
          <div className="rv-system-note">
            <span className="rv-signal" />
            <span>
              {__DEMO__ ? "Deterministic fixture" : "Rust / SQLite authority"}
            </span>
          </div>
          <p>One Plan. Many Agents.</p>
          <small>Browser foundation · {__BUILD_ID__}</small>
        </div>
      </aside>
      <div className="rv-main-shell">
        <header className="rv-topbar">
          <IconButton
            label="Open navigation"
            text="Menu"
            icon="menu"
            variant="ghost"
            className="rv-mobile-nav-trigger"
            onClick={() => setNavOpen(true)}
          />
          <div className="rv-breadcrumb">
            <span>Workspace</span>
            <Icon name="chevron" />
            <strong>{project.split(" / ")[0]}</strong>
          </div>
          <div className="rv-topbar-actions">
            <span className="rv-mode-label">
              {__DEMO__
                ? "Synthetic environment"
                : "Authenticated read boundary"}
            </span>
            <Popover label={t("settings")} text="Display">
              <div className="rv-section-stack">
                <Select
                  label="Theme"
                  value={preferences.theme}
                  onChange={(next) =>
                    setPreferences({
                      ...preferences,
                      theme: next as Preferences["theme"],
                    })
                  }
                  options={[
                    { value: "dark", label: "Dark" },
                    { value: "light", label: "Light" },
                    { value: "system", label: "System" },
                  ]}
                />
                <Select
                  label="Locale"
                  value={preferences.locale}
                  onChange={(next) =>
                    setPreferences({
                      ...preferences,
                      locale: next as Preferences["locale"],
                    })
                  }
                  options={[
                    { value: "en-US", label: "English · US" },
                    { value: "en-GB", label: "English · UK" },
                    ...(__DEMO__
                      ? [
                          { value: "qps-ploc", label: "QA · expanded labels" },
                          { value: "qps-rtl", label: "QA · RTL labels" },
                        ]
                      : []),
                  ]}
                />
                <Switch
                  label="Reduce motion"
                  checked={preferences.motion === "reduce"}
                  onChange={(e) =>
                    setPreferences({
                      ...preferences,
                      motion: e.target.checked ? "reduce" : "system",
                    })
                  }
                />
                <p className="rv-help">
                  Font and icon resources are external. System fonts and text
                  labels remain usable when unavailable.
                </p>
                <p className="rv-help">
                  Final logo assets are not verified. PWA icon certification
                  remains blocked.
                </p>
                {__DEMO__ && (
                  <Button onClick={() => setGallery(true)}>
                    UI Elements gallery
                  </Button>
                )}
              </div>
            </Popover>
            <Menu
              label="Session"
              items={[
                {
                  label: "Human login unavailable",
                  disabled: true,
                  onSelect: () => undefined,
                },
                {
                  label: "Clear local view",
                  onSelect: () => {
                    setSnapshot(null);
                    setHidden(true);
                    setToast(
                      "Local view cleared. This does not claim server-side logout.",
                    );
                  },
                },
              ]}
            />
          </div>
        </header>
        {__DEMO__ && (
          <div className="rv-demo-banner">
            <span>
              <strong>DEMO WORKSPACE</strong> Synthetic accounts, execution and
              costs. No real operations.
            </span>
            <div>
              <Button
                variant="ghost"
                icon="refresh"
                onClick={() => {
                  setRefresh((n) => n + 1);
                  setToast("Synthetic scenario reset.");
                }}
              >
                Reset
              </Button>
              <Button
                variant="secondary"
                icon="arrow"
                disabled={
                  !snapshot ||
                  hidden ||
                  snapshot.tasks.find((x) => x.id === "B")?.status ===
                    "verified"
                }
                onClick={() => {
                  if (snapshot) {
                    setSnapshot(advanceDemo(snapshot));
                    setToast(
                      "Synthetic evidence advanced. No controller authority was changed.",
                    );
                  }
                }}
              >
                Advance scenario
              </Button>
            </div>
          </div>
        )}
        <ConnectivityBanner
          state={availability}
          asOf={snapshot?.asOf ?? null}
          onRefresh={() => setRefresh((n) => n + 1)}
        />
        {update?.available && (
          <div className="rv-update" role="status">
            <span>A new static application shell is available.</span>
            <Button onClick={update.apply}>Reload to update</Button>
          </div>
        )}
        <main id="main" className="rv-page" tabIndex={-1}>
          {projects.length > 0 && !hidden && (
            <div className="rv-project-control">
              <ProjectSelector
                projects={projects}
                value={route.project ?? projects[0]!.id}
                onChange={(id) => {
                  setInspector(false);
                  navigate({ section: route.section, project: id, task: null });
                }}
              />
              <div>
                <Badge icon="shield">
                  {snapshot?.role ?? "Checking membership"}
                </Badge>
                {task && (
                  <Button variant="ghost" onClick={() => setInspector(true)}>
                    Selected task {task.id}
                  </Button>
                )}
              </div>
            </div>
          )}
          {loading ? (
            <Skeleton />
          ) : !snapshot || hidden ? (
            <>
              <PageHeader
                section="CONNECTED CONTROL"
                title="Your control plane, clearly connected."
                description="The browser never substitutes its own authority for trusted Rust authentication, membership or financial approval."
              />
              <EmptyState
                title={
                  availability === "unauthorized"
                    ? "Human authentication required"
                    : availability === "forbidden"
                      ? "Project access denied"
                      : hidden
                        ? "Local view cleared"
                        : "Browser integration is not activated"
                }
              >
                <span>
                  {availability === "unauthorized"
                    ? "A verified human browser session is required. Worker credentials cannot sign in here."
                    : availability === "forbidden"
                      ? "No private project data is displayed."
                      : "The current backend provides secure library services, but its trusted human browser transport is not operationally verified."}
                </span>
              </EmptyState>
              <p className="rv-note">
                Production command submission and live event streaming remain
                unavailable. Use the separately built, explicitly marked demo
                for interactive synthetic exploration.
              </p>
            </>
          ) : route.section === "projects" ? (
            <Projects
              snapshot={snapshot}
              selected={route.task}
              onSelect={selectTask}
            />
          ) : route.section === "execution" ? (
            <Execution
              snapshot={snapshot}
              selected={route.task}
              onSelect={selectTask}
            />
          ) : route.section === "costs-approvals" ? (
            <Costs snapshot={snapshot} />
          ) : route.section === "connections" ? (
            <Connections snapshot={snapshot} />
          ) : (
            <Results
              key={route.task}
              snapshot={snapshot}
              selected={route.task}
              onSelect={selectTask}
            />
          )}
          {!loading && availability === "unavailable" && snapshot && (
            <ErrorState title="Capability unavailable">
              The last snapshot is not a statement of current authority.
            </ErrorState>
          )}
          <footer className="rv-page-footer">
            <span>Rolvya / Connected Control</span>
            <span>
              {__DEMO__
                ? "Synthetic fixture · no live credentials"
                : "Server authority required for every privileged effect"}
            </span>
          </footer>
        </main>
      </div>
      <Drawer
        open={navOpen}
        onClose={() => setNavOpen(false)}
        title="Navigation"
      >
        {navigation}
      </Drawer>
      <Drawer
        open={inspector && Boolean(task)}
        onClose={() => setInspector(false)}
        title={t("inspector")}
      >
        {task && snapshot && (
          <TaskInspector
            task={task}
            snapshot={snapshot}
            onNavigate={chooseSection}
          />
        )}
      </Drawer>
      {__DEMO__ && (
        <Drawer
          open={gallery}
          onClose={() => setGallery(false)}
          title="UI Elements · development only"
        >
          <Gallery />
        </Drawer>
      )}
      {toast && <Toast message={toast} onDismiss={() => setToast("")} />}
    </div>
  );
}
