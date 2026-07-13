"use client";

import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";

type Task = {
  id: number;
  title: string;
  completed: boolean;
  created_at: string;
  updated_at: string;
};

type Filter = "all" | "active" | "done";

const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_URL}${path}`, {
    credentials: "include",
    ...options,
    headers: { "Content-Type": "application/json", ...options?.headers },
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ message: "Failed to connect" }));
    throw new Error(body.message ?? "Failed to connect");
  }
  if (response.status === 204) return undefined as T;
  return response.json();
}

export default function Home() {
  const [checking, setChecking] = useState(true);
  const [loggedIn, setLoggedIn] = useState(false);
  const [password, setPassword] = useState("");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [title, setTitle] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const loadTasks = useCallback(async () => {
    const data = await api<Task[]>("/api/tasks");
    setTasks(data);
  }, []);

  useEffect(() => {
    api<{ authenticated: boolean }>('/api/session')
      .then(async ({ authenticated }) => {
        setLoggedIn(authenticated);
        if (authenticated) await loadTasks();
      })
      .catch(() => setLoggedIn(false))
      .finally(() => setChecking(false));
  }, [loadTasks]);

  const visibleTasks = useMemo(() => {
    if (filter === "active") return tasks.filter((task) => !task.completed);
    if (filter === "done") return tasks.filter((task) => task.completed);
    return tasks;
  }, [filter, tasks]);

  const remaining = tasks.filter((task) => !task.completed).length;

  async function login(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      await api("/api/login", { method: "POST", body: JSON.stringify({ password }) });
      setLoggedIn(true);
      setPassword("");
      await loadTasks();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Failed to log in");
    } finally {
      setBusy(false);
    }
  }

  async function logout() {
    await api("/api/logout", { method: "POST" });
    setLoggedIn(false);
    setTasks([]);
  }

  async function addTask(event: FormEvent) {
    event.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) return;
    setBusy(true);
    setError("");
    try {
      const task = await api<Task>("/api/tasks", {
        method: "POST",
        body: JSON.stringify({ title: trimmed }),
      });
      setTasks((current) => [task, ...current]);
      setTitle("");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Failed to add task");
    } finally {
      setBusy(false);
    }
  }

  async function toggleTask(task: Task) {
    const updated = await api<Task>(`/api/tasks/${task.id}`, {
      method: "PATCH",
      body: JSON.stringify({ completed: !task.completed }),
    });
    setTasks((current) => current.map((item) => (item.id === updated.id ? updated : item)));
  }

  async function removeTask(id: number) {
    await api(`/api/tasks/${id}`, { method: "DELETE" });
    setTasks((current) => current.filter((task) => task.id !== id));
  }

  if (checking) {
    return <main className="center"><div className="loader" aria-label="Loading" /></main>;
  }

  if (!loggedIn) {
    return (
      <main className="login-shell">
        <section className="login-copy">
          <span className="eyebrow">TASKFLOW</span>
          <h1>Keep your tasks<br />simple and clear.</h1>
          <p>Start without hesitation and clear them when you're done. A simple task manager that makes daily work feel lighter.</p>
        </section>
        <section className="login-card" aria-labelledby="login-title">
          <div className="mark">T</div>
          <h2 id="login-title">Welcome back</h2>
          <p>Enter your password to continue.</p>
          <form onSubmit={login}>
            <label htmlFor="password">Password</label>
            <input id="password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" required autoFocus />
            {error && <div className="error" role="alert">{error}</div>}
            <button className="primary" disabled={busy}>{busy ? "Checking…" : "Log in"}</button>
          </form>
        </section>
      </main>
    );
  }

  return (
    <main className="app-shell">
      <header>
        <div className="brand"><span className="mark small">T</span><span>Taskflow</span></div>
        <button className="text-button" onClick={logout}>Log out</button>
      </header>

      <section className="workspace">
        <div className="heading-row">
          <div>
            <span className="eyebrow">MY TASKS</span>
            <h1>Today's tasks</h1>
            <p>{remaining === 0 ? "All done. Great work." : `Only ${remaining} left. Let's keep going.`}</p>
          </div>
          <div className="count"><strong>{remaining}</strong><span>Remaining</span></div>
        </div>

        <form className="task-form" onSubmit={addTask}>
          <input value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Enter a new task…" maxLength={255} aria-label="New task" />
          <button className="primary" disabled={busy || !title.trim()}>Add</button>
        </form>

        {error && <div className="error" role="alert">{error}</div>}

        <div className="toolbar">
          <div className="filters" aria-label="Filter tasks">
            {(["all", "active", "done"] as Filter[]).map((item) => (
              <button key={item} className={filter === item ? "active" : ""} onClick={() => setFilter(item)}>
                {{ all: "All", active: "Active", done: "Done" }[item]}
              </button>
            ))}
          </div>
          <span>{tasks.length} tasks</span>
        </div>

        <div className="task-list">
          {visibleTasks.length === 0 ? (
            <div className="empty"><div>✓</div><h2>No tasks yet</h2><p>You can add tasks from the input field above.</p></div>
          ) : visibleTasks.map((task) => (
            <article className={`task ${task.completed ? "completed" : ""}`} key={task.id}>
              <button className="check" onClick={() => toggleTask(task)} aria-label={task.completed ? `Mark ${task.title} as active` : `Mark ${task.title} as done`}>{task.completed ? "✓" : ""}</button>
              <span>{task.title}</span>
              <button className="delete" onClick={() => removeTask(task.id)} aria-label={`Delete ${task.title}`}>Delete</button>
            </article>
          ))}
        </div>
      </section>
    </main>
  );
}
