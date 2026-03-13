package app.fastxt.android

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.recyclerview.widget.RecyclerView
import org.json.JSONObject
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Activity showing notes grouped by AI-generated categories.
 */
class CategoryActivity : AppCompatActivity() {

    private lateinit var adapter: CategoryAdapter
    private val categories = mutableListOf<CategoryItem>()
    private val scope = CoroutineScope(Dispatchers.Main)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_category)

        supportActionBar?.title = "Categories"

        adapter = CategoryAdapter(categories) { note ->
            // Open note detail
            val intent = android.content.Intent(this, ItemDetailActivity::class.java).apply {
                putExtra(ItemDetailFragment.ARG_NOTE_ID, note.id)
                putExtra(ItemDetailFragment.ARG_NOTE_TEXT, note.txt)
                putExtra(ItemDetailFragment.ARG_NOTE_TAGS, note.tags)
            }
            startActivity(intent)
        }

        findViewById<RecyclerView>(R.id.category_list).adapter = adapter

        // Organize button
        findViewById<View>(R.id.organize_button).setOnClickListener {
            organizeNotes()
        }

        loadCategories()
    }

    private fun loadCategories() {
        scope.launch {
            try {
                val response = withContext(Dispatchers.IO) {
                    RustBridge.search("", 0)
                }

                val json = JSONObject(response)
                val notesArray = json.getJSONArray("notes")

                // Group notes by category
                val grouped = mutableMapOf<String, MutableList<Note>>()

                for (i in 0 until notesArray.length()) {
                    val noteJson = notesArray.getJSONObject(i)
                    val note = Note.fromJson(noteJson)
                    val category = note.aiCategory?.takeIf { it.isNotEmpty() } ?: "uncategorized"

                    grouped.getOrPut(category) { mutableListOf() }.add(note)
                }

                // Convert to list for adapter
                categories.clear()
                grouped.forEach { (category, notes) ->
                    categories.add(CategoryItem(category, notes))
                }

                // Sort: uncategorized at the end
                categories.sortWith { a, b ->
                    if (a.name == "uncategorized") 1
                    else if (b.name == "uncategorized") -1
                    else a.name.compareTo(b.name)
                }

                adapter.notifyDataSetChanged()
            } catch (e: Exception) {
                // Handle error
            }
        }
    }

    private fun organizeNotes() {
        scope.launch {
            try {
                findViewById<View>(R.id.organize_button).isEnabled = false
                findViewById<TextView>(R.id.organize_status).text = "Organizing..."

                val response = withContext(Dispatchers.IO) {
                    RustBridge.run("{\"action\":\"ai-organize\",\"limit\":100}")
                }

                val json = JSONObject(response)
                val processed = json.optInt("processed", 0)
                val errors = json.optInt("errors", 0)

                findViewById<TextView>(R.id.organize_status).text =
                    "Organized $processed notes ($errors errors)"

                // Reload categories
                loadCategories()
            } catch (e: Exception) {
                findViewById<TextView>(R.id.organize_status).text =
                    "Error: ${e.message}"
            } finally {
                findViewById<View>(R.id.organize_button).isEnabled = true
            }
        }
    }

    data class CategoryItem(val name: String, val notes: List<Note>)

    class CategoryAdapter(
        private val items: List<CategoryItem>,
        private val onNoteClick: (Note) -> Unit
    ) : RecyclerView.Adapter<CategoryAdapter.ViewHolder>() {

        override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): ViewHolder {
            val view = LayoutInflater.from(parent.context)
                .inflate(R.layout.category_item, parent, false)
            return ViewHolder(view)
        }

        override fun onBindViewHolder(holder: ViewHolder, position: Int) {
            val item = items[position]
            holder.categoryName.text = "${item.name.capitalize()} (${item.notes.size})"
            holder.categoryIcon.text = getCategoryIcon(item.name)

            // Show up to 5 notes per category
            val notesText = item.notes.take(5).joinToString("\n") { note ->
                "• ${note.txt.take(50)}${if (note.txt.length > 50) "..." else ""}"
            }
            if (item.notes.size > 5) {
                holder.notesPreview.text = notesText + "\n... and ${item.notes.size - 5} more"
            } else {
                holder.notesPreview.text = notesText
            }

            holder.itemView.setOnClickListener {
                // Could expand to show all notes in category
                onNoteClick(item.notes.first())
            }
        }

        override fun getItemCount() = items.size

        private fun getCategoryIcon(category: String): String {
            return when (category.lowercase()) {
                "work" -> "💼"
                "personal" -> "👤"
                "reference" -> "📚"
                "idea" -> "💡"
                "task" -> "✅"
                "other" -> "⋯"
                else -> "📁"
            }
        }

        inner class ViewHolder(view: View) : RecyclerView.ViewHolder(view) {
            val categoryIcon: TextView = view.findViewById(R.id.category_icon)
            val categoryName: TextView = view.findViewById(R.id.category_name)
            val notesPreview: TextView = view.findViewById(R.id.notes_preview)
        }
    }
}
