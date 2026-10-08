--- The running clock, as the server reports it (`org-lsp/clock`), for the
--- statusline: what Emacs shows in its mode line.
local M = {}

---@class org_lsp.ClockState
---@field heading string the clocked entry's headline
---@field minutes integer minutes clocked on it when the server reported
---@field effort? integer its effort, in minutes
---@field uri string
---@field line integer
---@field received integer when the report came (`vim.uv.now()`)

---@type org_lsp.ClockState?
M.state = nil

local timer

local function duration(minutes)
  return ('%d:%02d'):format(math.floor(minutes / 60), minutes % 60)
end

--- What Emacs's mode line shows while a clock runs, such as
--- `[0:25/1:00] (Write the report)`: the time clocked on the entry, its
--- effort, and its headline. Empty when no clock runs.
---@return string
function M.status()
  local state = M.state
  if not state then
    return ''
  end
  local minutes = state.minutes + math.floor((vim.uv.now() - state.received) / 60000)
  local time = state.effort and duration(minutes) .. '/' .. duration(state.effort) or duration(minutes)
  return ('[%s] (%s)'):format(time, state.heading)
end

--- The server's report: the clock that runs, or none.
---@param params table|nil
function M.update(params)
  if params and params ~= vim.NIL and params.heading then
    M.state = {
      heading = params.heading,
      minutes = params.minutes or 0,
      effort = params.effort ~= vim.NIL and params.effort or nil,
      uri = params.uri,
      line = params.line,
      received = vim.uv.now(),
    }
  else
    M.state = nil
  end
  -- The statusline shows the minutes as they pass.
  if M.state and not timer then
    timer = vim.uv.new_timer()
    timer:start(60000, 60000, vim.schedule_wrap(function()
      vim.cmd('redrawstatus!')
    end))
  elseif not M.state and timer then
    timer:stop()
    timer:close()
    timer = nil
  end
  vim.cmd('redrawstatus!')
  vim.api.nvim_exec_autocmds('User', { pattern = 'OrgLspClock', modeline = false })
end

---@class org_lsp.TimerState
---@field seconds integer its value when the report came
---@field countdown boolean whether it counts down
---@field paused boolean
---@field received integer when the report came (`vim.uv.now()`)

--- The relative timer or the countdown (`org-lsp/timer`).
---@type org_lsp.TimerState?
M.timer = nil

local ticking

local function hms(seconds)
  return ('%d:%02d:%02d'):format(math.floor(seconds / 3600), math.floor(seconds % 3600 / 60), seconds % 60)
end

--- What Emacs's mode line shows while the relative timer or a countdown is
--- on (`org-timer-update-mode-line`), such as `<0:05:23>`. Empty when none
--- is.
---@return string
function M.timer_status()
  local state = M.timer
  if not state then
    return ''
  end
  local seconds = state.seconds
  if not state.paused then
    local passed = math.floor((vim.uv.now() - state.received) / 1000)
    seconds = state.countdown and math.max(0, seconds - passed) or seconds + passed
  end
  return ('<%s>'):format(hms(seconds))
end

--- The server's report: the timer that runs, or none.
---@param params table|nil
function M.update_timer(params)
  if params and params ~= vim.NIL and params.seconds then
    M.timer = {
      seconds = params.seconds,
      countdown = params.countdown == true,
      paused = params.paused == true,
      received = vim.uv.now(),
    }
  else
    M.timer = nil
  end
  -- The statusline shows the seconds as they pass.
  local running = M.timer ~= nil and not M.timer.paused
  if running and not ticking then
    ticking = vim.uv.new_timer()
    ticking:start(1000, 1000, vim.schedule_wrap(function()
      vim.cmd('redrawstatus!')
    end))
  elseif not running and ticking then
    ticking:stop()
    ticking:close()
    ticking = nil
  end
  vim.cmd('redrawstatus!')
end

return M
