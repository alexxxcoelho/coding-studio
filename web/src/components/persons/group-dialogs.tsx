// Dialogs of the groups: create, rename / move / describe, and delete with the
// reassignment of the sub-groups and members when the group is not empty.

import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { toast } from "sonner"

import { Alerts, Field, NativeSelect, TEXTAREA } from "@/components/settings/fields"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { t } from "@/i18n"
import { flattenGroups, invalidateRegister, personsApi, subtreeIds, type GroupNode } from "@/lib/persons-api"
import { plainError } from "@/lib/persons-errors"

/** Options of a parent or target select: the path of each group, indented by depth. */
function GroupOptions({ groups }: { groups: GroupNode[] }) {
  return (
    <>
      {groups.map((g) => (
        <option key={g.group_id} value={g.group_id}>
          {`${"  ".repeat(g.depth)}${g.name}`}
        </option>
      ))}
    </>
  )
}

/** Creates a group (`group` null) or edits one: name, parent, description. */
export function GroupDialog({
  open,
  onClose,
  tree,
  group,
  defaultParent,
  onSaved,
}: {
  open: boolean
  onClose: () => void
  tree: GroupNode[]
  group: GroupNode | null
  defaultParent: string | null
  onSaved: (id: string) => void
}) {
  const qc = useQueryClient()
  const [name, setName] = useState(group?.name ?? "")
  const [parent, setParent] = useState<string>(group ? (group.parent_id ?? "") : (defaultParent ?? ""))
  const [description, setDescription] = useState(group?.description ?? "")
  const [error, setError] = useState<string | null>(null)
  const flat = flattenGroups(tree)
  const forbidden = new Set(group ? subtreeIds(group) : [])
  const parents = flat.filter((g) => !forbidden.has(g.group_id))

  const save = useMutation({
    mutationFn: async () => {
      const body = { name: name.trim(), parent_id: parent === "" ? null : parent, description: description.trim() }
      return group ? personsApi.updateGroup(group.group_id, body) : personsApi.createGroup(body)
    },
    onSuccess: async (g) => {
      await invalidateRegister(qc)
      toast.success(group ? t("persons.group_saved") : t("persons.group_created", { name: g.name }))
      onSaved(g.group_id)
      onClose()
    },
    onError: (e) => setError(plainError(e)),
  })

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{group ? t("persons.edit_group") : t("persons.new_group")}</DialogTitle>
          <DialogDescription>{t("persons.group_dialog_help")}</DialogDescription>
        </DialogHeader>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            setError(null)
            save.mutate()
          }}
        >
          <Field label={t("persons.group_name")}>
            <Input value={name} autoFocus onChange={(e) => setName(e.target.value)} />
          </Field>
          <Field label={t("persons.parent_group")}>
            <NativeSelect value={parent} onChange={(e) => setParent(e.target.value)}>
              <option value="">{t("persons.top_level")}</option>
              <GroupOptions groups={parents} />
            </NativeSelect>
          </Field>
          <Field label={t("persons.group_description")}>
            <textarea className={TEXTAREA} rows={3} value={description} onChange={(e) => setDescription(e.target.value)} />
          </Field>
          {error && <Alerts messages={[error]} />}
          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t("common.annuler")}
            </Button>
            <Button type="submit" disabled={!name.trim() || save.isPending}>
              {group ? t("common.enregistrer") : t("persons.create_group")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

/** Deletes a group. A group that has sub-groups or members needs another group to take them over. */
export function DeleteGroupDialog({
  open,
  onClose,
  tree,
  group,
  onDeleted,
}: {
  open: boolean
  onClose: () => void
  tree: GroupNode[]
  group: GroupNode
  onDeleted: () => void
}) {
  const qc = useQueryClient()
  const [target, setTarget] = useState("")
  const [error, setError] = useState<string | null>(null)
  const empty = group.children.length === 0 && group.n_persons === 0
  const below = new Set(subtreeIds(group))
  const targets = flattenGroups(tree).filter((g) => !below.has(g.group_id))

  const remove = useMutation({
    mutationFn: () => personsApi.deleteGroup(group.group_id, empty ? undefined : target),
    onSuccess: async () => {
      await invalidateRegister(qc)
      toast.success(t("persons.group_deleted", { name: group.name }))
      onDeleted()
      onClose()
    },
    onError: (e) => setError(plainError(e)),
  })

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("persons.delete_group_title", { name: group.name })}</DialogTitle>
          <DialogDescription>
            {empty ? t("persons.delete_group_empty") : t("persons.delete_group_reassign", { groups: group.children.length, persons: group.n_persons })}
          </DialogDescription>
        </DialogHeader>
        {!empty &&
          (targets.length === 0 ? (
            <p className="small l2">{t("persons.no_target_group")}</p>
          ) : (
            <Field label={t("persons.reassign_to")}>
              <NativeSelect value={target} onChange={(e) => setTarget(e.target.value)}>
                <option value="">{t("persons.choose_group")}</option>
                <GroupOptions groups={targets} />
              </NativeSelect>
            </Field>
          ))}
        {error && <Alerts messages={[error]} />}
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("common.annuler")}
          </Button>
          <Button variant="destructive" disabled={remove.isPending || (!empty && target === "")} onClick={() => remove.mutate()}>
            {t("persons.delete_group")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
