// {{name}}'s home: the team's notes, newest first, with one to add and each to tick off.

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link } from "react-router";
import { api, bootstrap } from "@ferrum/client";
import { greeting } from "./greeting";

interface Note {
  id: string;
  title: string;
  status: "Open" | "Done";
}

const ENTITY = "{{entity}}";

export function Notes() {
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const notes = useQuery({
    queryKey: ["notes"],
    queryFn: ({ signal }) => api.get<{ rows: Note[] }>(`/api/v1/${ENTITY}`, { limit: 100, fields: "title,status", sort: "created_at desc" }, signal),
  });
  const refresh = () => void qc.invalidateQueries({ queryKey: ["notes"] });
  const add = useMutation({ mutationFn: (t: string) => api.post(`/api/v1/${ENTITY}`, { title: t }), onSuccess: () => (setTitle(""), refresh()) });
  const tick = useMutation({ mutationFn: (n: Note) => api.patch(`/api/v1/${ENTITY}/${encodeURIComponent(n.id)}`, { status: n.status === "Done" ? "Open" : "Done" }), onSettled: refresh });
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (title.trim()) add.mutate(title.trim());
  };
  return (
    <main id="main" className="mx-auto grid max-w-2xl gap-4 p-4 sm:p-6">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">{{name}}</h1>
        <p className="text-muted-foreground">{greeting(bootstrap())}</p>
      </header>
      <form onSubmit={submit} className="flex gap-2">
        <input className="input flex-1" placeholder="A new note" aria-label="A new note" maxLength={140} value={title} onChange={(e) => setTitle(e.target.value)} />
        <button type="submit" className="btn btn-primary btn-md" disabled={add.isPending}>
          Add
        </button>
      </form>
      {(notes.error || add.error) && <p role="alert" className="text-destructive">{String((notes.error ?? add.error)?.message)}</p>}
      <ul className="divide-y rounded-xl border bg-card">
        {notes.data?.rows.length === 0 && <li className="p-4 text-muted-foreground">No notes yet: add the first one above.</li>}
        {notes.data?.rows.map((n) => (
          <li key={n.id} className="flex items-center gap-3 p-3">
            <input type="checkbox" checked={n.status === "Done"} onChange={() => tick.mutate(n)} aria-label={`Done: ${n.title}`} />
            <Link to={`/e/${ENTITY}/${encodeURIComponent(n.id)}`} className="flex-1 hover:underline">
              {n.title}
            </Link>
          </li>
        ))}
      </ul>
    </main>
  );
}
