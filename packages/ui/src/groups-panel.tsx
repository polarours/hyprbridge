import { useState } from "react";
import { Button, Panel } from "./primitives";

export interface Group {
  id: string;
  name: string;
  members: string[];
}

export interface GroupsPanelProps {
  groups: Group[];
  onCreateGroup: (name: string) => void;
  onDeleteGroup: (id: string) => void;
  onAddMember: (groupId: string, deviceId: string) => void;
  onRemoveMember: (groupId: string, deviceId: string) => void;
}

export function GroupsPanel(props: GroupsPanelProps) {
  const [newGroupName, setNewGroupName] = useState("");
  const [selectedGroup, setSelectedGroup] = useState<string | null>(null);
  const [newMemberId, setNewMemberId] = useState("");

  function handleCreate() {
    if (newGroupName.trim()) {
      props.onCreateGroup(newGroupName.trim());
      setNewGroupName("");
    }
  }

  return (
    <Panel title="Device Groups" eyebrow="Team">
      <div className="ib-groups">
        <div className="ib-groups__create">
          <input
            className="ib-groups__input"
            type="text"
            value={newGroupName}
            onChange={(e) => setNewGroupName(e.target.value)}
            placeholder="New group name..."
          />
          <Button tone="accent" onClick={handleCreate}>Create</Button>
        </div>
        {props.groups.length > 0 ? (
          <div className="ib-groups__list">
            {props.groups.map((group) => (
              <div
                key={group.id}
                className={`ib-groups__group ${selectedGroup === group.id ? "ib-groups__group--selected" : ""}`}
                onClick={() => setSelectedGroup(group.id)}
              >
                <div className="ib-groups__group-header">
                  <strong>{group.name}</strong>
                  <span>{group.members.length} members</span>
                </div>
                {selectedGroup === group.id ? (
                  <div className="ib-groups__group-details">
                    <div className="ib-groups__members">
                      {group.members.map((member) => (
                        <div key={member} className="ib-groups__member">
                          <span>{member.slice(0, 8)}...</span>
                          <Button tone="neutral" onClick={() => props.onRemoveMember(group.id, member)}>×</Button>
                        </div>
                      ))}
                    </div>
                    <div className="ib-groups__add-member">
                      <input
                        className="ib-groups__input"
                        type="text"
                        value={newMemberId}
                        onChange={(e) => setNewMemberId(e.target.value)}
                        placeholder="Device ID..."
                      />
                      <Button tone="accent" onClick={() => { props.onAddMember(group.id, newMemberId); setNewMemberId(""); }}>Add</Button>
                    </div>
                    <Button tone="danger" onClick={() => props.onDeleteGroup(group.id)}>Delete Group</Button>
                  </div>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <p className="ib-copy">No groups created yet.</p>
        )}
      </div>
    </Panel>
  );
}
