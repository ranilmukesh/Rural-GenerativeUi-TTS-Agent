import pandas as pd
import networkx as nx
from collections import Counter
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import io
import base64

class RoutingEngine:
    def __init__(self, data_path):
        self.df = pd.read_csv(data_path)
        self.df.columns = self.df.columns.str.strip()
        self.G = nx.Graph()
        self.all_unique_skills = set()
        self.all_jobs = set()
        self._build_graph()

    def _build_graph(self):
        self.df['All_Skills'] = (
            self.df['Skills'].fillna('') + "," +
            self.df.get('Programming Languages', pd.Series([''] * len(self.df))) + "," +
            self.df['Tools'].fillna('')
        )
        # Split skills and explode
        df_exploded = self.df.assign(All_Skills=self.df['All_Skills'].str.split(',')).explode('All_Skills')
        df_exploded['All_Skills'] = df_exploded['All_Skills'].str.strip()
        # Remove empty strings
        df_exploded = df_exploded[df_exploded['All_Skills'] != '']
        
        for _, row in df_exploded.iterrows():
            job = row['Job roles']
            skill = row['All_Skills']
            if isinstance(skill, str) and len(skill) > 1:
                self.G.add_edge(job, skill)
                self.all_unique_skills.add(skill)
                self.all_jobs.add(job)

    def get_skill_list(self):
        return sorted([str(s) for s in self.all_unique_skills if isinstance(s, str)])
        
    def get_job_list(self):
        return sorted(list(self.all_jobs))

    def get_gap(self, target_job, user_skills):
        if target_job not in self.G.nodes:
            return []
        
        required_skills = set(self.G.neighbors(target_job))
        current_skills = set(user_skills)
        missing_skills = list(required_skills - current_skills)
        
        return missing_skills

    def recommend(self, user_skills):
        possible_jobs = []
        for skill in user_skills:
            if skill in self.G.nodes:
                neighbors = list(self.G.neighbors(skill))
                possible_jobs.extend(neighbors)

        if not possible_jobs:
            return None, []

        top_jobs_counter = Counter(possible_jobs).most_common(1)
        best_job = top_jobs_counter[0][0]
        missing_skills = self.get_gap(best_job, user_skills)
        return best_job, missing_skills

    def get_career_transition_path(self, current_job, target_job):
        """Finds the shortest upskilling path between two roles via shared skills."""
        if current_job not in self.G.nodes or target_job not in self.G.nodes:
            return None

        try:
            # NetworkX finds the shortest path alternating: Job -> Skill -> Job
            path = nx.shortest_path(self.G, source=current_job, target=target_job)

            skills_to_learn = [node for node in path if node in self.all_unique_skills]
            stepping_stones = [node for node in path if node in self.all_jobs and node not in (current_job, target_job)]

            return {
                "path": path,
                "skills_to_learn": skills_to_learn,
                "stepping_stones": stepping_stones
            }
        except Exception:
            return None

    def get_subgraph_figure_base64(self, center_node, user_skills, depth=1):
        try:
            if center_node not in self.G.nodes: return None
            skills = list(self.G.neighbors(center_node))
            subG = self.G.subgraph([center_node] + skills)
            plt.figure(figsize=(8, 6), facecolor='#0f172a')
            pos = nx.spring_layout(subG, seed=42, k=0.5)
            nx.draw_networkx_edges(subG, pos, edge_color='#334155', alpha=0.5)
            
            node_colors = []
            for n in subG.nodes():
                if n == center_node:
                    node_colors.append('#ffffff')
                elif n in user_skills:
                    node_colors.append('#047857')
                else:
                    node_colors.append('#dc2626')
                    
            nx.draw_networkx_nodes(subG, pos, node_color=node_colors, node_size=600, alpha=0.9)
            labels = {n: n for n in subG.nodes()}
            nx.draw_networkx_labels(subG, pos, labels=labels, font_size=8, font_color='white', font_family='sans-serif')
            plt.title(f"Skill Gap Map: {center_node}", color='white')
            plt.axis('off')
            
            buf = io.BytesIO()
            plt.savefig(buf, format='png', bbox_inches='tight', facecolor='#0f172a')
            plt.close()
            buf.seek(0)
            img_base64 = base64.b64encode(buf.read()).decode('utf-8')
            return f"data:image/png;base64,{img_base64}"
        except Exception as e:
            print(f"Graph Error: {e}")
            return None
