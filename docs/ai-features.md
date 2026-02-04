# AI Features

Clepho integrates with local and cloud LLMs to provide AI-powered photo descriptions and semantic search capabilities.

## Overview

AI features in Clepho:

1. **Photo Descriptions** - Generate natural language descriptions of photos
2. **Semantic Search** - Find photos by describing what you're looking for
3. **Batch Processing** - Process entire directories automatically

## LLM Setup

### Supported Providers

| Provider | Type | Cost | Setup |
|----------|------|------|-------|
| LM Studio | Local | Free | Download app + model |
| Ollama | Local | Free | Install + pull model |
| OpenAI | Cloud | Paid | API key required |
| Anthropic | Cloud | Paid | API key required |

### LM Studio Setup

1. Download [LM Studio](https://lmstudio.ai/)
2. Download a vision-capable model:
   - Search for "llava" or "vision" models
   - Recommended: `llava-1.5-7b` or `bakllava`
3. Start the local server:
   - Click "Local Server" tab
   - Click "Start Server"
   - Default: `http://127.0.0.1:1234`

4. Configure Clepho:
   ```toml
   [llm]
   provider = "lmstudio"
   endpoint = "http://127.0.0.1:1234/v1"
   model = "llava-1.5-7b"
   ```

### Ollama Setup

1. Install [Ollama](https://ollama.ai/)
   ```bash
   curl -fsSL https://ollama.ai/install.sh | sh
   ```

2. Pull a vision model:
   ```bash
   ollama pull llava
   # or for better quality:
   ollama pull llava:13b
   ```

3. Start Ollama (usually automatic):
   ```bash
   ollama serve
   ```

4. Configure Clepho:
   ```toml
   [llm]
   provider = "ollama"
   endpoint = "http://127.0.0.1:11434"
   model = "llava"
   ```

### OpenAI Setup

1. Get API key from [OpenAI Platform](https://platform.openai.com/)

2. Configure Clepho:
   ```toml
   [llm]
   provider = "openai"
   endpoint = "https://api.openai.com/v1"
   model = "gpt-4-vision-preview"
   api_key = "sk-..."
   ```

### Anthropic Setup

1. Get API key from [Anthropic Console](https://console.anthropic.com/)

2. Configure Clepho:
   ```toml
   [llm]
   provider = "anthropic"
   endpoint = "https://api.anthropic.com"
   model = "claude-3-sonnet-20240229"
   api_key = "sk-ant-..."
   ```

## Generating Descriptions

### Single Photo

1. Navigate to a photo
2. Press `i` to describe with AI
3. Wait for processing
4. Description appears in preview pane

```
Generating description for photo_001.jpg...
Done: "A golden retriever playing fetch on a sandy beach..."
```

### Batch Processing

Process all photos in current directory:

1. Press `I` (uppercase) for batch processing
2. Progress shown in status bar: `[B:45%]`
3. View task details with `T`

```
Batch processing 150 photos...
[████████████████░░░░░░░░░░░░░░░░░░░░░░░] 45% (68/150)
```

### Scheduled Processing

Schedule batch processing for later:

1. Press `@` to open schedule dialog
2. Select "LLM Batch Process"
3. Set date/time
4. Press Enter

## Description Content

### What's Analyzed

The LLM examines:
- **Scene content** - Objects, people, animals, landscape
- **Actions** - What's happening in the photo
- **Setting** - Indoor/outdoor, time of day, weather
- **Mood** - Emotional tone of the image
- **Technical aspects** - Composition, lighting

### Example Descriptions

```
"A family of four having a picnic in a sunny park. The parents
are seated on a red checkered blanket while two children play
with a frisbee nearby. Large oak trees provide dappled shade,
and a lake is visible in the background."
```

```
"Close-up macro photograph of a honeybee collecting pollen from
a bright purple lavender flower. The bee's fuzzy body and
translucent wings are in sharp focus against a soft, blurred
green background."
```

## Customizing the Prompt

There are two ways to customize the LLM prompt: `custom_prompt` adds context to the default prompt, and `base_prompt` replaces the default prompt entirely.

### `custom_prompt` — Add Context

Add `custom_prompt` to your `[llm]` section in `config.toml`:

```toml
[llm]
provider = "lmstudio"
endpoint = "http://127.0.0.1:1234/v1"
model = "llava-1.5-7b"

# Custom context prepended to the default prompt
custom_prompt = "These are photos from a wedding in June 2024."
```

Your custom prompt is prepended as context to the base prompt:

```
Context: [your custom_prompt here]

Describe this image in detail. Include information about:
1) The main subject or scene
2) Notable objects, people, or elements
3) Colors, lighting, and mood
4) Any text visible in the image
Keep the description concise but informative.

After the description, on a new line write TAGS: followed by a comma-separated
list of relevant tags for organizing this photo.
```

#### Example Custom Prompts

| Goal | Custom Prompt |
|------|---------------|
| Shorter descriptions | `"Keep responses under 50 words."` |
| People focus | `"Focus on describing people: their appearance, expressions, and actions."` |
| Technical details | `"Focus on camera settings, composition, and photographic technique."` |
| Specific context | `"These are photos from a wedding in June 2024."` |

### `base_prompt` — Replace the Default Prompt

If you need full control over what the LLM is asked to do, set `base_prompt` to replace the built-in prompt entirely:

```toml
[llm]
base_prompt = """
Describe this image concisely in 2-3 sentences. No introductions or preambles.
If people are present, focus on them: who they appear to be, what they're doing, expressions.
If no people, describe the location and notable objects.

After the description, on a new line write TAGS: followed by comma-separated tags.
"""
```

**Important:** Your base prompt must include instructions for the `TAGS:` line, since tag parsing depends on that format. If you omit it, tags will not be extracted.

When both `custom_prompt` and `base_prompt` are set, `custom_prompt` is still prepended as context to your custom base prompt.

### Per-Folder Prompts

Instead of a single global prompt, you can set a different custom prompt for each directory. When you trigger a **Scan** (`s`), **Describe with LLM** (`i`), or **Batch LLM** (`I`) action, the confirmation dialog includes an editable text field showing the current directory's prompt.

- **Edit the prompt** before confirming to tailor descriptions for that folder
- **Leave it blank** to fall back to the global `custom_prompt` from your config
- Per-folder prompts are **stored in the database** (in the `directory_prompts` table) and persist across sessions
- The **daemon** also uses per-folder prompts when processing scheduled LLM batch tasks
- Press **Tab** in the confirmation dialog to switch focus between the prompt field and the confirm buttons

This is useful when different directories contain different types of photos (e.g., wedding photos vs. nature photography) and benefit from different context.

#### Example Base Prompts

| Goal | Base Prompt |
|------|-------------|
| Minimal tagging | `"List comma-separated tags for this image. Write TAGS: followed by the tags."` |
| People-focused | `"Describe who is in this photo, what they look like, and what they are doing. Then write TAGS: followed by relevant tags."` |
| Location-focused | `"Describe where this photo was taken, including any landmarks or notable features. Then write TAGS: followed by relevant tags."` |

## Semantic Search

Search your photo collection using natural language.

### Starting a Search

1. Press `/` to open search dialog
2. Type your search query
3. Press Enter to search
4. Navigate results with `j`/`k`
5. Press Enter to go to selected photo

### Search Examples

| Query | Finds |
|-------|-------|
| "beach sunset" | Photos described as beach sunsets |
| "dog playing" | Photos with dogs in action |
| "birthday party" | Celebration photos |
| "mountain landscape" | Scenic mountain photos |
| "people laughing" | Candid happy moments |

### Search Dialog

```
┌─────────────────────────────────────────────────────────────┐
│ Search: birthday cake                                       │
├─────────────────────────────────────────────────────────────┤
│ Results (12 matches):                                       │
│                                                             │
│ > [92%] IMG_4521.jpg - Birthday celebration with...        │
│   [87%] party_2024.jpg - Children gathered around...       │
│   [85%] celebration.jpg - A decorated chocolate...         │
│   [71%] family_dinner.jpg - Family seated at table...      │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ j/k:navigate Enter:go to photo Esc:close                   │
└─────────────────────────────────────────────────────────────┘
```

### Search Scoring

Results are ranked by relevance:
- **90-100%** - Strong match
- **70-90%** - Good match
- **50-70%** - Partial match
- **Below 50%** - Weak match (usually filtered)

## Embeddings

For semantic search, Clepho uses text embeddings:

### How It Works

1. Photo descriptions are converted to vectors (embeddings)
2. Search queries are converted to vectors
3. Cosine similarity finds closest matches

### Embedding Storage

Embeddings are stored in the database:
- ~1.5KB per photo
- Enables instant search
- No LLM needed for searching

### Without Embeddings

If embeddings aren't configured, search falls back to:
- Text matching on descriptions
- Less accurate but still useful

## Processing Status

### Status Indicators

| Indicator | Meaning |
|-----------|---------|
| `[L:...]` | Single LLM description in progress |
| `[B:45%]` | Batch processing at 45% |
| No indicator | No AI processing active |

### Task List

Press `T` to view detailed task status:

```
┌─────────────────────────────────────────────────────────────┐
│ Running Tasks                                               │
├─────────────────────────────────────────────────────────────┤
│ > LLM Batch Process - 45% (68/150) - vacation/photo_068.jpg│
│   Elapsed: 5:32                                            │
├─────────────────────────────────────────────────────────────┤
│ c:cancel task  Esc:close                                   │
└─────────────────────────────────────────────────────────────┘
```

## Performance

### Processing Speed

| Provider | Speed | Quality |
|----------|-------|---------|
| LM Studio (7B) | ~5 sec/photo | Good |
| LM Studio (13B) | ~15 sec/photo | Better |
| Ollama (llava) | ~3 sec/photo | Good |
| OpenAI GPT-4V | ~2 sec/photo | Excellent |

### Resource Usage

**Local LLMs:**
- GPU: 4-8GB VRAM recommended
- CPU: Slower but works
- RAM: 8-16GB for 7B models

**Cloud APIs:**
- Minimal local resources
- Network bandwidth for images
- API costs per image

## Best Practices

### Model Selection

| Use Case | Recommended Model |
|----------|-------------------|
| Quick processing | llava (7B) |
| Better descriptions | llava:13b |
| Best quality | GPT-4 Vision |
| Privacy-focused | Local models only |

### Batch Processing Tips

1. **Start small** - Test with a few photos first
2. **Schedule overnight** - Large batches take time
3. **Check quality** - Verify descriptions are useful
4. **Re-process selectively** - Only re-do poor descriptions

### Search Optimization

1. **Be specific** - "red car on highway" > "car"
2. **Use natural language** - Write like you'd describe it
3. **Try variations** - "dog", "puppy", "canine"
4. **Combine concepts** - "sunset beach palm trees"

## Troubleshooting

### LLM Connection Failed

```
Error: Failed to connect to LLM endpoint
```

**Solutions:**
- Verify LM Studio/Ollama is running
- Check endpoint URL in config
- Check firewall settings
- Test with: `curl http://127.0.0.1:1234/v1/models`

### Slow Processing

**Local models:**
- Use GPU if available
- Try smaller model
- Close other GPU applications

**Cloud APIs:**
- Check internet connection
- Verify API quota
- Try during off-peak hours

### Poor Descriptions

**Solutions:**
- Try a larger/better model
- Ensure image isn't corrupted
- Check image isn't too small
- Cloud APIs generally more accurate

### Search Not Finding Photos

- Ensure photos have descriptions (`i` first)
- Check search terms match description style
- Try broader search terms
- Verify embeddings were generated
