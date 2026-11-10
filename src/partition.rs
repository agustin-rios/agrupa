//! Quién está en qué grupo. No sabe nada de costos.

#[derive(Debug, Clone)]
pub struct Partition {
    group_of: Vec<Option<usize>>,
    members: Vec<Vec<usize>>,
    /// Posición de cada alumno dentro de los integrantes de su grupo, para sacarlo en O(1).
    slot: Vec<usize>,
}

impl Partition {
    /// `group_of[alumno]` = su grupo, o `None` si todavía no tiene.
    pub fn new(group_count: usize, group_of: Vec<Option<usize>>) -> Partition {
        let mut members = vec![Vec::new(); group_count];
        let mut slot = vec![0; group_of.len()];
        for (student, group) in group_of.iter().enumerate() {
            if let Some(group) = *group {
                slot[student] = members[group].len();
                members[group].push(student);
            }
        }
        Partition {
            group_of,
            members,
            slot,
        }
    }

    /// Partición donde todos tienen grupo.
    pub fn complete(group_count: usize, groups: &[usize]) -> Partition {
        Partition::new(group_count, groups.iter().map(|&g| Some(g)).collect())
    }

    pub fn group_count(&self) -> usize {
        self.members.len()
    }

    pub fn student_count(&self) -> usize {
        self.group_of.len()
    }

    pub fn group_of(&self, student: usize) -> Option<usize> {
        self.group_of[student]
    }

    /// Grupo de cada alumno.
    pub fn assignment(&self) -> &[Option<usize>] {
        &self.group_of
    }

    pub fn members(&self, group: usize) -> &[usize] {
        &self.members[group]
    }

    pub fn size(&self, group: usize) -> usize {
        self.members[group].len()
    }

    pub fn sizes(&self) -> Vec<usize> {
        self.members.iter().map(Vec::len).collect()
    }

    pub fn smallest_size(&self) -> usize {
        self.members.iter().map(Vec::len).min().unwrap_or(0)
    }

    /// Alumnos sin grupo, en orden.
    pub fn unassigned(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.student_count()).filter(|&s| self.group_of[s].is_none())
    }

    /// Cambia de grupo a un alumno (`None` = lo deja sin grupo).
    pub fn move_student(&mut self, student: usize, to: Option<usize>) {
        if let Some(from) = self.group_of[student] {
            let slot = self.slot[student];
            self.members[from].swap_remove(slot);
            if let Some(&moved) = self.members[from].get(slot) {
                self.slot[moved] = slot;
            }
        }
        if let Some(to) = to {
            self.slot[student] = self.members[to].len();
            self.members[to].push(student);
        }
        self.group_of[student] = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_keep_members_and_sizes_in_sync() {
        let mut p = Partition::new(2, vec![Some(0), Some(0), Some(1), None]);
        p.move_student(0, Some(1));
        p.move_student(3, Some(0));
        p.move_student(2, None);
        assert_eq!(p.sizes(), vec![2, 1]);
        let mut first: Vec<usize> = p.members(0).to_vec();
        first.sort_unstable();
        assert_eq!(first, vec![1, 3]);
        assert_eq!(p.members(1), &[0]);
        assert_eq!(p.unassigned().collect::<Vec<_>>(), vec![2]);
    }
}
